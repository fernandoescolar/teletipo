use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tracing::{debug, warn};

use crate::backend::PtyBackend;
use crate::error::PtyError;

type Result<T> = std::result::Result<T, PtyError>;

/// Maximum number of read chunks (each up to [`READ_CHUNK_SIZE`] bytes) that
/// may sit in the PTY → consumer channel before the reader thread blocks.
///
/// This caps in-flight memory at roughly `PTY_CHANNEL_CAPACITY * READ_CHUNK_SIZE`
/// bytes per session (currently ~256 KiB). When the renderer falls behind, the
/// reader blocks on `send`, which lets the kernel PTY buffer fill up and apply
/// natural backpressure to the child process — preferable to unbounded queueing
/// that would grow memory until the consumer catches up.
const PTY_CHANNEL_CAPACITY: usize = 64;

/// Size of the buffer the reader thread fills before forwarding a chunk.
const READ_CHUNK_SIZE: usize = 4096;

pub struct PortablePtySession {
    writer: Box<dyn Write + Send>,
    /// Master end of the PTY; kept alive for resize calls.
    master: Box<dyn portable_pty::MasterPty>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    rx: Receiver<Vec<u8>>,
    queued_chunks: Arc<AtomicUsize>,
    /// Handle to the reader thread, retained so `Drop` can join with a
    /// timeout. `Option` so the join can take ownership in `drop`.
    reader_handle: Option<thread::JoinHandle<()>>,
}

// ── Shell integration ─────────────────────────────────────────────────────────

struct IntegrationSetup {
    extra_args: Vec<String>,
    env_vars: Vec<(String, String)>,
}

fn gui_shell_path() -> String {
    let current =
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin:/usr/sbin:/sbin".to_string());
    format!(
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:{}",
        current
    )
}

fn default_term() -> String {
    std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string())
}

/// Writes per-shell integration scripts that make the shell emit
/// `ESC ] 133 ; D ; <exit_code> BEL` before each prompt (OSC 133).
/// Returns `None` if the shell is not supported or the files cannot be written.
#[tracing::instrument(skip(shell))]
#[allow(clippy::too_many_lines)]
fn setup_shell_integration(shell: &str) -> Option<IntegrationSetup> {
    let shell_name = std::path::Path::new(shell)
        .file_name()
        .and_then(|n| n.to_str())?;

    let home = match std::env::var("HOME") {
        Ok(home) => home,
        Err(err) => {
            warn!(shell = %shell, error = %err, "shell integration unavailable: HOME not set");
            return None;
        }
    };
    let integration_dir = std::path::PathBuf::from(&home)
        .join(".config")
        .join("teletipo")
        .join("shell-integration");

    if let Err(err) = std::fs::create_dir_all(&integration_dir) {
        warn!(path = %integration_dir.display(), error = %err, "shell integration unavailable: failed to create integration directory");
        return None;
    }

    let precmd_hook = r#"_ret=$?; printf '\033]133;D;%d\007' "$_ret"; printf '\033]133;A\007'; printf '\033]7;file://%s%s\007' "$(hostname -f 2>/dev/null || hostname)" "$PWD""#;
    let preexec_hook = r#"printf '\033]133;B\007'; printf '\033]133;C\007'"#;
    let fish_precmd_hook = r#"set -l _ret $status; printf '\033]133;D;%d\007' $_ret; printf '\033]133;A\007'; printf '\033]7;file://%s%s\007' (hostname -f 2>/dev/null; or hostname) $PWD"#;
    let fish_preexec_hook = r#"printf '\033]133;B\007'; printf '\033]133;C\007'"#;

    match shell_name {
        "zsh" => {
            // The real dotfile directory (respects a pre-existing ZDOTDIR).
            let real_zdotdir = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.clone());
            let gui_path = gui_shell_path();

            // .zshenv – sourced for every zsh invocation (non-interactive too).
            let zshenv = format!(
                "# Teletipo shell integration\n\
                 [ -f '{real_zdotdir}/.zshenv' ] && source '{real_zdotdir}/.zshenv'\n\
                 export PATH='{gui_path}'\n"
            );
            if let Err(err) = std::fs::write(integration_dir.join(".zshenv"), zshenv) {
                warn!(path = %integration_dir.join(".zshenv").display(), error = %err, "failed to write zsh shell integration file");
                return None;
            }

            // .zshrc – sourced for interactive shells.
            let zshrc = format!(
                "# Teletipo shell integration\n\
                 autoload -Uz add-zsh-hook\n\
                 _teletipo_precmd() {{ {precmd_hook}; }}\n\
                 _teletipo_preexec() {{ {preexec_hook}; }}\n\
                 add-zsh-hook precmd _teletipo_precmd\n\
                 add-zsh-hook preexec _teletipo_preexec\n\
                 # Restore normal dotfile lookup for subshells.\n\
                 unset ZDOTDIR\n\
                 [ -f '{real_zdotdir}/.zshrc' ] && source '{real_zdotdir}/.zshrc'\n"
            );
            if let Err(err) = std::fs::write(integration_dir.join(".zshrc"), zshrc) {
                warn!(path = %integration_dir.join(".zshrc").display(), error = %err, "failed to write zsh shell integration file");
                return None;
            }

            Some(IntegrationSetup {
                extra_args: vec![],
                env_vars: vec![
                    (
                        "ZDOTDIR".to_string(),
                        integration_dir.to_string_lossy().into_owned(),
                    ),
                    ("PATH".to_string(), gui_path),
                ],
            })
        }
        "bash" => {
            let gui_path = gui_shell_path();
            // bash --rcfile lets us inject a custom init without --noprofile/--norc.
            let bashrc = format!(
                "# Teletipo shell integration\n\
                 [ -f '{home}/.bashrc' ] && source '{home}/.bashrc'\n\
                 _teletipo_precmd() {{ {precmd_hook}; }}\n\
                 _teletipo_dbg() {{ [ \"$BASH_COMMAND\" != '_teletipo_precmd' ] && printf '\\033]133;B\\007' && printf '\\033]133;C\\007'; }}\n\
                 trap '_teletipo_dbg' DEBUG\n\
                 PROMPT_COMMAND=\"${{PROMPT_COMMAND:+${{PROMPT_COMMAND}}; }}_teletipo_precmd\"\n"
            );
            let rcfile = integration_dir.join(".bashrc");
            if let Err(err) = std::fs::write(&rcfile, bashrc) {
                warn!(path = %rcfile.display(), error = %err, "failed to write bash shell integration file");
                return None;
            }

            Some(IntegrationSetup {
                extra_args: vec![
                    "--rcfile".to_string(),
                    rcfile.to_string_lossy().into_owned(),
                ],
                env_vars: vec![("PATH".to_string(), gui_path)],
            })
        }
        "fish" => {
            // fish supports `--init-command` (`-C`) for running an inline script
            // before the interactive session — the cleanest injection point that
            // does not require overriding XDG_CONFIG_HOME or writing to the user's
            // own fish config directory.
            let gui_path = gui_shell_path();
            let fish_init = format!(
                "# Teletipo shell integration\n\
                 function _teletipo_precmd --on-event fish_prompt\n\
                     {fish_precmd_hook}\n\
                 end\n\
                 function _teletipo_preexec --on-event fish_preexec\n\
                     {fish_preexec_hook}\n\
                 end\n"
            );
            let fish_file = integration_dir.join("teletipo.fish");
            if let Err(err) = std::fs::write(&fish_file, fish_init) {
                warn!(path = %fish_file.display(), error = %err, "failed to write fish shell integration file");
                return None;
            }

            let source_cmd = format!("source '{}'", fish_file.to_string_lossy());
            Some(IntegrationSetup {
                extra_args: vec!["--init-command".to_string(), source_cmd],
                env_vars: vec![("PATH".to_string(), gui_path)],
            })
        }
        _ => None,
    }
}

// ── Session ───────────────────────────────────────────────────────────────────

impl PortablePtySession {
    /// Spawns the given shell with shell-integration hooks injected so the shell
    /// emits OSC 133;D;<exit_code> before each prompt.
    ///
    /// Returns `(session, integration_active)`.  When `integration_active` is
    /// `false` the shell does not emit OSC 133 and exit-code tracking is
    /// unavailable (fallback: all commands are saved to history).
    #[tracing::instrument(skip(shell, cwd))]
    pub fn spawn_shell(
        shell: &str,
        rows: u16,
        cols: u16,
        cwd: Option<&str>,
    ) -> Result<(Self, bool)> {
        let integration = setup_shell_integration(shell);
        let baseline_path = gui_shell_path();
        let term = default_term();

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::stage("open pty", e))?;

        let mut cmd = CommandBuilder::new(shell);

        // Finder-launched app sessions on macOS may inherit a sparse env.
        // Keep a deterministic baseline so full-screen terminal apps (vim, less)
        // can rely on TERM and command lookup consistently.
        cmd.env("PATH", &baseline_path);
        cmd.env("TERM", &term);

        if let Some(ref setup) = integration {
            for arg in &setup.extra_args {
                cmd.arg(arg);
            }
            for (key, val) in &setup.env_vars {
                cmd.env(key, val);
            }
        }

        debug!(shell = %shell, term = %term, integration_active = integration.is_some(), "spawning shell with normalized pty environment");

        if let Some(dir) = cwd
            && !dir.is_empty()
        {
            cmd.cwd(dir);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::stage("spawn command", e))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::stage("clone pty reader", e))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::stage("take pty writer", e))?;

        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(PTY_CHANNEL_CAPACITY);
        let queued_chunks = Arc::new(AtomicUsize::new(0));
        let queued_chunks_for_thread = Arc::clone(&queued_chunks);
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; READ_CHUNK_SIZE];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = tx.send(buf[..n].to_vec()) {
                            debug!(error = %err, "pty reader exiting: receiver dropped");
                            break;
                        } else {
                            let depth =
                                queued_chunks_for_thread.fetch_add(1, Ordering::Relaxed) + 1;
                            metrics::gauge!("pty_channel_depth").set(depth as f64);
                            metrics::counter!("pty_read_bytes").increment(n as u64);
                        }
                    }
                    Err(err) => {
                        warn!(error = %err, "pty reader exiting on read error");
                        break;
                    }
                }
            }
        });

        let session = Self {
            writer,
            master: pair.master,
            child,
            rx,
            queued_chunks: Arc::clone(&queued_chunks),
            reader_handle: Some(reader_handle),
        };
        Ok((session, integration.is_some()))
    }

    #[tracing::instrument(skip(program, args, cwd))]
    pub fn spawn_command(
        program: &str,
        args: &[&str],
        rows: u16,
        cols: u16,
        cwd: Option<&str>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::stage("open pty", e))?;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(*arg);
        }
        cmd.env("TERM", default_term());
        if let Some(dir) = cwd
            && !dir.is_empty()
        {
            cmd.cwd(dir);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::stage("spawn command", e))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::stage("clone pty reader", e))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::stage("take pty writer", e))?;

        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(PTY_CHANNEL_CAPACITY);
        let queued_chunks = Arc::new(AtomicUsize::new(0));
        let queued_chunks_for_thread = Arc::clone(&queued_chunks);
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; READ_CHUNK_SIZE];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = tx.send(buf[..n].to_vec()) {
                            debug!(error = %err, "pty reader exiting: receiver dropped");
                            break;
                        } else {
                            let depth =
                                queued_chunks_for_thread.fetch_add(1, Ordering::Relaxed) + 1;
                            metrics::gauge!("pty_channel_depth").set(depth as f64);
                            metrics::counter!("pty_read_bytes").increment(n as u64);
                        }
                    }
                    Err(err) => {
                        warn!(error = %err, "pty reader exiting on read error");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            writer,
            master: pair.master,
            child,
            rx,
            queued_chunks: Arc::clone(&queued_chunks),
            reader_handle: Some(reader_handle),
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>> {
        self.child
            .try_wait()
            .map_err(|e| PtyError::stage("query child status", e))
    }

    /// Returns the OS process-id of the spawned child, if available.
    pub fn child_pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// Returns the foreground process-group of the slave PTY (the value of
    /// `tcgetpgrp` on the slave fd).  When this differs from `child_pid()`
    /// the shell has spawned a child that is currently in the foreground
    /// (e.g. `vim`, `sudo`, a long-running script).
    ///
    /// Always returns `None` on Windows.
    pub fn foreground_pgrp(&self) -> Option<i32> {
        #[cfg(unix)]
        {
            use portable_pty::MasterPty;
            MasterPty::process_group_leader(&*self.master)
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Returns `true` when the slave PTY's foreground process group is a
    /// child of the spawned shell — i.e. the shell is currently waiting on
    /// a foreground command (vim, sudo, ssh, a script, …) rather than
    /// displaying its prompt.  Used by the UI to bypass the command editor
    /// and route keystrokes directly to the running program.
    pub fn foreground_child_running(&self) -> bool {
        match (self.foreground_pgrp(), self.child_pid()) {
            (Some(fg), Some(shell)) => fg as u32 != shell,
            _ => false,
        }
    }

    /// Notifies the PTY child process of a terminal size change (sends SIGWINCH).
    #[tracing::instrument(skip(self))]
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if let Err(err) = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            warn!(rows, cols, error = %err, "failed to resize pty");
        }
    }
}

impl PtyBackend for PortablePtySession {
    fn write_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    fn try_read_output(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        let mut total = 0usize;
        while let Ok(chunk) = self.rx.try_recv() {
            total += chunk.len();
            let depth = self
                .queued_chunks
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            metrics::gauge!("pty_channel_depth").set(depth as f64);
            out.extend_from_slice(&chunk);
        }
        Ok(total)
    }
}

impl Drop for PortablePtySession {
    fn drop(&mut self) {
        if let Err(err) = self.child.kill() {
            warn!(error = %err, "failed to kill pty child on drop");
        }
        if let Err(err) = self.child.wait() {
            warn!(error = %err, "failed to wait for pty child on drop");
        }
        // REL-3: join the reader thread with a short timeout. Killing the child
        // closes the master FD, so `reader.read` should return EOF promptly; we
        // poll `is_finished` to avoid blocking shutdown forever if the OS
        // doesn't deliver the close in time.
        if let Some(handle) = self.reader_handle.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
            loop {
                if handle.is_finished() {
                    if let Err(err) = handle.join() {
                        warn!(?err, "pty reader thread panicked");
                    }
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    warn!("pty reader thread did not exit within 250ms; detaching");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "windows"))]
    use super::PortablePtySession;
    #[cfg(not(target_os = "windows"))]
    use super::gui_shell_path;
    #[cfg(not(target_os = "windows"))]
    use crate::backend::PtyBackend;
    #[cfg(not(target_os = "windows"))]
    use std::time::{Duration, Instant};

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn gui_shell_path_prefers_homebrew_prefixes() {
        let path = gui_shell_path();
        assert!(
            path.starts_with(
                "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:"
            )
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn spawn_command_delivers_output() {
        let mut session =
            PortablePtySession::spawn_command("sh", &["-lc", "printf hi"], 24, 80, None)
                .expect("spawn command");

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut output = Vec::new();
        while Instant::now() < deadline {
            session
                .try_read_output(&mut output)
                .expect("read pty output");
            if String::from_utf8_lossy(&output).contains("hi") {
                return;
            }
            if session.try_wait().expect("query child status").is_some() {
                break;
            }
            std::thread::yield_now();
        }

        assert!(String::from_utf8_lossy(&output).contains("hi"));
    }
}

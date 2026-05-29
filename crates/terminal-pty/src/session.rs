use std::io::{self, Read, Write};
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
}

// ── Shell integration ─────────────────────────────────────────────────────────

struct IntegrationSetup {
    extra_args: Vec<String>,
    env_vars: Vec<(String, String)>,
}

/// Writes per-shell integration scripts that make the shell emit
/// `ESC ] 133 ; D ; <exit_code> BEL` before each prompt (OSC 133).
/// Returns `None` if the shell is not supported or the files cannot be written.
#[tracing::instrument(skip(shell))]
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

    let hook = r#"printf '\033]133;D;%d\007' "$?""#;

    match shell_name {
        "zsh" => {
            // The real dotfile directory (respects a pre-existing ZDOTDIR).
            let real_zdotdir = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.clone());

            // .zshenv – sourced for every zsh invocation (non-interactive too).
            let zshenv = format!(
                "# Teletipo shell integration\n\
                 [ -f '{real_zdotdir}/.zshenv' ] && source '{real_zdotdir}/.zshenv'\n"
            );
            if let Err(err) = std::fs::write(integration_dir.join(".zshenv"), zshenv) {
                warn!(path = %integration_dir.join(".zshenv").display(), error = %err, "failed to write zsh shell integration file");
                return None;
            }

            // .zshrc – sourced for interactive shells.
            let zshrc = format!(
                "# Teletipo shell integration\n\
                 _teletipo_precmd() {{ {hook}; }}\n\
                 precmd_functions+=(_teletipo_precmd)\n\
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
                env_vars: vec![(
                    "ZDOTDIR".to_string(),
                    integration_dir.to_string_lossy().into_owned(),
                )],
            })
        }
        "bash" => {
            // bash --rcfile lets us inject a custom init without --noprofile/--norc.
            let bashrc = format!(
                "# Teletipo shell integration\n\
                 [ -f '{home}/.bashrc' ] && source '{home}/.bashrc'\n\
                 _teletipo_precmd() {{ {hook}; }}\n\
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
                env_vars: vec![],
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

        if let Some(ref setup) = integration {
            for arg in &setup.extra_args {
                cmd.arg(arg);
            }
            for (key, val) in &setup.env_vars {
                cmd.env(key, val);
            }
        }

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
        thread::spawn(move || {
            let mut buf = [0u8; READ_CHUNK_SIZE];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = tx.send(buf[..n].to_vec()) {
                            debug!(error = %err, "pty reader exiting: receiver dropped");
                            break;
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
        thread::spawn(move || {
            let mut buf = [0u8; READ_CHUNK_SIZE];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = tx.send(buf[..n].to_vec()) {
                            debug!(error = %err, "pty reader exiting: receiver dropped");
                            break;
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
    }
}

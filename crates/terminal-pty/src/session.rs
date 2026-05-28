use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use anyhow::Context;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::backend::PtyBackend;

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
fn setup_shell_integration(shell: &str) -> Option<IntegrationSetup> {
    let shell_name = std::path::Path::new(shell)
        .file_name()
        .and_then(|n| n.to_str())?;

    let home = std::env::var("HOME").ok()?;
    let integration_dir = std::path::PathBuf::from(&home)
        .join(".config")
        .join("teletipo")
        .join("shell-integration");

    std::fs::create_dir_all(&integration_dir).ok()?;

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
            std::fs::write(integration_dir.join(".zshenv"), zshenv).ok()?;

            // .zshrc – sourced for interactive shells.
            let zshrc = format!(
                "# Teletipo shell integration\n\
                 _teletipo_precmd() {{ {hook}; }}\n\
                 precmd_functions+=(_teletipo_precmd)\n\
                 # Restore normal dotfile lookup for subshells.\n\
                 unset ZDOTDIR\n\
                 [ -f '{real_zdotdir}/.zshrc' ] && source '{real_zdotdir}/.zshrc'\n"
            );
            std::fs::write(integration_dir.join(".zshrc"), zshrc).ok()?;

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
            std::fs::write(&rcfile, bashrc).ok()?;

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
    pub fn spawn_shell(
        shell: &str,
        rows: u16,
        cols: u16,
        cwd: Option<&str>,
    ) -> anyhow::Result<(Self, bool)> {
        let integration = setup_shell_integration(shell);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .context("open pty")?;

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

        let child = pair.slave.spawn_command(cmd).context("spawn command")?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let session = Self { writer, master: pair.master, child, rx };
        Ok((session, integration.is_some()))
    }

    pub fn spawn_command(
        program: &str,
        args: &[&str],
        rows: u16,
        cols: u16,
        cwd: Option<&str>,
    ) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(*arg);
        }
        if let Some(dir) = cwd
            && !dir.is_empty() {
                cmd.cwd(dir);
            }

        let child = pair.slave.spawn_command(cmd).context("spawn command")?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self { writer, master: pair.master, child, rx })
    }

    pub fn try_wait(&mut self) -> anyhow::Result<Option<portable_pty::ExitStatus>> {
        self.child.try_wait().context("query child status")
    }

    /// Returns the OS process-id of the spawned child, if available.
    pub fn child_pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// Notifies the PTY child process of a terminal size change (sends SIGWINCH).
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
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
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

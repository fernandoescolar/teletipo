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

impl PortablePtySession {
    pub fn spawn_shell(shell: &str, rows: u16, cols: u16, cwd: Option<&str>) -> anyhow::Result<Self> {
        Self::spawn_command(shell, &[], rows, cols, cwd)
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

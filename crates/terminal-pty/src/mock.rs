use std::io;

use crate::backend::PtyBackend;

#[derive(Default)]
pub struct MockPty {
    input_log: Vec<u8>,
    output_buffer: Vec<u8>,
}

impl MockPty {
    pub fn push_output(&mut self, bytes: &[u8]) {
        self.output_buffer.extend_from_slice(bytes);
    }

    pub fn input_log(&self) -> &[u8] {
        &self.input_log
    }
}

impl PtyBackend for MockPty {
    fn write_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.input_log.extend_from_slice(bytes);
        Ok(())
    }

    fn try_read_output(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        let n = self.output_buffer.len();
        out.extend_from_slice(&self.output_buffer);
        self.output_buffer.clear();
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::MockPty;
    use crate::backend::PtyBackend;

    #[test]
    fn mock_pty_roundtrip() {
        let mut pty = MockPty::default();
        pty.write_input(b"ls\n").expect("write");
        pty.push_output(b"file_a\n");

        let mut out = Vec::new();
        let n = pty.try_read_output(&mut out).expect("read");

        assert_eq!(n, 7);
        assert_eq!(pty.input_log(), b"ls\n");
        assert_eq!(out, b"file_a\n");
    }
}

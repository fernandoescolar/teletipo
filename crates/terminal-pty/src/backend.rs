use std::io;

pub trait PtyBackend {
    fn write_input(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn try_read_output(&mut self, out: &mut Vec<u8>) -> io::Result<usize>;
}

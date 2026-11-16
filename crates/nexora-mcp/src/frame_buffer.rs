use std::io::{self, Write};
pub(crate) struct FrameBuffer(pub Vec<u8>);
impl Write for FrameBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 1_048_576 {
            return Err(io::Error::other("MCP response limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

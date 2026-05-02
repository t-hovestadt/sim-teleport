use super::MapError;

/// Non-Windows stub. `open()` always fails; `create()` returns a heap buffer.
pub struct MockSharedMap {
    buf: Vec<u8>,
}

impl MockSharedMap {
    pub fn open(_name: &str) -> Result<Self, MapError> {
        Err(MapError::Unavailable)
    }

    pub fn create(_name: &str, size: usize) -> Result<Self, MapError> {
        Ok(Self {
            buf: vec![0u8; size],
        })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    pub fn size(&self) -> usize {
        self.buf.len()
    }
}

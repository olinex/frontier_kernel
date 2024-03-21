// @author:    olinex
// @time:      2024/01/10

// self mods

// use other mods
use alloc::string::{String, ToString};
use alloc::vec::Vec;

// use self mods
use super::error::{KernelError, Result};

pub(crate) struct ByteBuffers {
    inner: Vec<&'static mut [u8]>,
    length: usize,
}
impl ByteBuffers {
    ///Create a `UserBuffer` by parameter
    pub(crate) fn new(inner: Vec<&'static mut [u8]>, length: usize) -> Self {
        Self { inner, length }
    }

    ///Length of `UserBuffer`
    pub(crate) fn len(&self) -> usize {
        self.length
    }

    pub(crate) fn into_slices(self) -> Vec<&'static mut [u8]> {
        self.inner
    }

    pub(crate) fn into_utf8_str(self) -> Result<String> {
        let mut bytes: Vec<u8> = Vec::new();
        for slice in self.inner.iter() {
            bytes.extend_from_slice(slice);
        }
        Ok(core::str::from_utf8(&bytes)?.to_string())
    }

    pub(crate) fn into_iter(self) -> ByteBuffersU8Iterator {
        ByteBuffersU8Iterator {
            buffer: self.inner,
            index: 0,
            offset: 0,
        }
    }
}

pub(crate) struct ByteBuffersU8Iterator {
    buffer: Vec<&'static mut [u8]>,
    index: usize,
    offset: usize,
}
impl ByteBuffersU8Iterator {
    pub(crate) fn next(&mut self) -> Option<u8> {
        if let Some(inner_buffer) = self.buffer.get(self.index) {
            if let Some(value) = inner_buffer.get(self.offset) {
                self.offset += 1;
                Some(*value)
            } else {
                self.index += 1;
                self.offset = 0;
                self.next()
            }
        } else {
            None
        }
    }

    pub(crate) fn next_mut(&mut self, byte: u8) -> Result<()> {
        if let Some(inner_buffer) = self.buffer.get_mut(self.index) {
            if let Some(value) = inner_buffer.get_mut(self.offset) {
                self.offset += 1;
                *value = byte;
                Ok(())
            } else {
                self.index += 1;
                self.offset = 0;
                self.next_mut(byte)
            }
        } else {
            Err(KernelError::EOB)
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum RingBufferStatus {
    EMPTY,
    FULL,
    NORMAL(usize),
}

pub(crate) struct RingBuffer {
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    buffer: Vec<u8>,
}
impl RingBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 2);
        Self {
            head: 0,
            tail: 0,
            status: RingBufferStatus::EMPTY,
            buffer: vec![0; capacity],
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self.status {
            RingBufferStatus::EMPTY => 0,
            RingBufferStatus::FULL => self.capacity(),
            RingBufferStatus::NORMAL(size) => size,
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.buffer.len()
    }

    pub(crate) fn read_byte(&mut self) -> Option<u8> {
        match self.len() {
            0 => None,
            size => {
                let byte = self.buffer[self.tail];
                if self.tail + 1 == self.capacity() {
                    self.tail = 0;
                } else {
                    self.tail += 1;
                }
                if self.tail == self.head {
                    self.status = RingBufferStatus::EMPTY;
                } else {
                    self.status = RingBufferStatus::NORMAL(size - 1);
                }
                Some(byte)
            }
        }
    }

    pub(crate) fn write_byte(&mut self, byte: u8) -> Result<()> {
        let capacity = self.capacity();
        match self.len() {
            size if size == capacity => Err(KernelError::EOB),
            size => {
                self.buffer[self.head] = byte;
                if self.head + 1 == capacity {
                    self.head = 0;
                } else {
                    self.head += 1;
                }
                if self.tail == self.head {
                    self.status = RingBufferStatus::FULL;
                } else {
                    self.status = RingBufferStatus::NORMAL(size + 1);
                };
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_ring_buffer_read_write_byte() {
        let mut buffer = RingBuffer::new(10);
        assert_eq!(0, buffer.len());

        for batch in 1..=10 {
            for _ in 0..batch {
                assert!(buffer.write_byte(batch).is_ok());
            }
            if batch == 10 {
                assert!(RingBufferStatus::FULL == buffer.status);
            }
            assert_eq!(batch as usize, buffer.len());
            for _ in 0..batch {
                assert!(buffer.read_byte().is_some_and(|byte| byte == batch));
            }
            assert!(RingBufferStatus::EMPTY == buffer.status);
            assert_eq!(0, buffer.len());
            assert!(buffer.read_byte().is_none())
        }
    }
}

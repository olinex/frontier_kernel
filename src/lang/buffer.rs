// @author:    olinex
// @time:      2024/01/10

// self mods

// use other mods
use alloc::string::String;
use alloc::vec::Vec;

// use self mods
use super::error::{KernelError, Result};

/// Transferring data between the kernel state and the user state
/// requires the address translation of the memory page,
/// and the conversion of the contiguous memory space to the kernel mode
/// may be split into multiple non-contiguous memory pages.
/// ByteBuffers encapsulate multiple memory pages passed by the user mode
/// and provide methods to make it easier for the kernel to access the data in the user mode.
pub(crate) struct ByteBuffers {
    /// Multiple non-contiguous memory pages, stored as byte slices.
    ///
    /// - TODO:
    /// When switching between kernel and user mode,
    /// we use the unsafe method that breaks the lifecycle of Rust,
    /// so we can't specify the lifecycle of the memory page pointer well.
    /// Looking forward to solving it in the future.
    inner: Vec<&'static mut [u8]>,
    length: usize,
}
impl ByteBuffers {
    /// Create a new byte buffers
    ///
    /// - Arguments
    ///     - inner: multiple non-contiguous memory pages
    ///     - length: total number of the bytes bassed by the user mode.
    pub(crate) fn new(inner: Vec<&'static mut [u8]>, length: usize) -> Self {
        Self { inner, length }
    }

    pub(crate) fn len(&self) -> usize {
        self.length
    }

    /// Sometimes we need to access these distiguous memory pages directly.
    /// This method directly unpacks and returns a list of memory page slices
    pub(crate) fn into_slices(self) -> Vec<&'static mut [u8]> {
        self.inner
    }

    /// Splice the discontinuous memory pages passed in by the user mode in the kernel space,
    /// and try to parse them into UTF8 encoded strings.
    ///
    /// - TODO:
    /// Splicing memory pages directly will perform memory copying, which is relatively inefficient.
    ///
    /// - Errors
    ///     - ParseUtf8Error
    pub(crate) fn into_utf8_str(self) -> Result<String> {
        let mut string = String::new();
        for slice in self.inner.iter() {
            for byte in slice.iter() {
                string.push(*byte as char)
            }
        }
        Ok(string)
    }

    /// Convert into iterator, which can read/write the discontinuous memory pages continuously.
    pub(crate) fn into_iter(self) -> ByteBuffersU8Iterator {
        ByteBuffersU8Iterator {
            buffer: self.inner,
            index: 0,
            offset: 0,
        }
    }
}

/// The iterator which can read/write the discontinuous memory pages continuously.
pub(crate) struct ByteBuffersU8Iterator {
    buffer: Vec<&'static mut [u8]>,
    index: usize,
    offset: usize,
}
impl ByteBuffersU8Iterator {
    /// Get the next byte continuously from the discontinuous memory pages.
    /// If return None, means no more bytes have not yet read
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

    /// Write byte into the discontinuous memory pages continuously.
    /// If return Err, means no more memory space have not yet write.
    ///
    /// - Arguments
    ///     - byte: the u8 value we want to write
    ///
    /// - Errors
    ///     - EOB
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

/// The available state of the ring buffer.
#[derive(Copy, Clone, PartialEq)]
pub(crate) enum RingBufferStatus {
    /// No more byte have not yet been readed
    EMPTY,
    /// No more space for more byte
    FULL,
    NORMAL(usize),
}

pub(crate) struct RingBuffer {
    /// The index position of the buffer,
    /// pointing to the byte that have not yet been read
    head: usize,

    /// The index position of the buffer,
    /// pointing to the byte that have not yet been written
    tail: usize,

    /// The available state of the ring buffer.
    status: RingBufferStatus,
    buffer: Vec<u8>,
}
impl RingBuffer {
    /// Create a new ring buffer
    ///
    /// - Arguments
    ///     - capacity: the length of the buffer in heap.
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

    /// Read the new byte from ring buffer
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

    /// Write the new byte into ring buffer
    ///
    /// - Arguments
    ///     - byte: byte to write to the end of ring buffer
    ///
    /// - Errors
    ///     - EOB
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

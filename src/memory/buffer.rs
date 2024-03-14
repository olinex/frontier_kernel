// @author:    olinex
// @time:      2024/01/10

// self mods

// use other mods
use alloc::vec::Vec;
use core::iter::{IntoIterator, Iterator};

// use self mods

pub(crate) struct ByteBuffers {
    ///U8 vec
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

    pub(crate) fn slices(&self) -> &Vec<&'static mut [u8]> {
        &self.inner
    }

    pub(crate) fn into_slices(self) -> Vec<&'static mut [u8]> {
        self.inner
    }
}
impl IntoIterator for ByteBuffers {
    type IntoIter = ByteBuffersU8Iterator;
    type Item = u8;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            buffer: self,
            index: 0,
            offset: 0,
        }
    }
}

pub(crate) struct ByteBuffersU8Iterator {
    buffer: ByteBuffers,
    index: usize,
    offset: usize,
}
impl Iterator for ByteBuffersU8Iterator {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        let inner_buffer = self.buffer.slices().get(self.index)?;
        if let Some(value) = inner_buffer.get(self.offset) {
            self.offset += 1;
            Some(*value)
        } else {
            self.index += 1;
            self.offset = 0;
            self.next()
        }
    }
}

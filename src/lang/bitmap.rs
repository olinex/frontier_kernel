// @author:    olinex
// @time:      2023/09/13

// self mods

// use other mods
use alloc::vec::Vec;

// use self mods
use super::container;
use super::error::*;

const BLOCK_BIT_SIZE: usize = 8;

/// BitMap
/// Save the bits as a vector of u8.
/// You can set or get each bit value as boolean.
pub(crate) struct BitMap {
    length: usize,
    map: container::UserPromiseRefCell<Vec<u8>>,
}
impl BitMap {
    /// Get the Quotient of the index which will be divided by 8
    /// - Arguments
    ///     - index: The index of the bit, starting from 0
    pub(crate) fn offset(index: usize) -> usize {
        index / BLOCK_BIT_SIZE
    }

    /// Get the Remainder of the index which will be divided by 8
    /// - Arguments
    ///     - index: The index of the bit, starting from 0
    pub(crate) fn limit(index: usize) -> usize {
        index % BLOCK_BIT_SIZE
    }

    /// Get the length of the vector of u8
    pub(crate) fn block_size(&self) -> usize {
        self.map.access().len()
    }

    /// Create a new BitMap according the length
    /// - Arguments
    ///     - length: The count of the bits
    pub(crate) fn new(length: usize) -> Self {
        let map: Vec<u8> = vec![0; (length + (BLOCK_BIT_SIZE - 1)) / 8];
        Self {
            length,
            map: unsafe { container::UserPromiseRefCell::new(map) },
        }
    }

    /// Get the bit value pointed to by the index
    /// - Arguments
    ///     - index: The index of the bit, starting from 0
    ///
    /// - Returns
    ///     - boolean: get the bit boolean value
    /// 
    /// - Errors
    ///     - IndexOutOfRange
    pub(crate) fn get_bit(&self, index: usize) -> Result<bool> {
        match (
            index < self.length,
            self.map.access().get(Self::offset(index)),
        ) {
            (true, Some(block)) => Ok((block & (1u8 << Self::limit(index))) > 0),
            _ => Err(KernelError::IndexOutOfRange {
                index,
                start: 0,
                end: self.length,
            }),
        }
    }

    /// Set the bit value pointed to by the index
    /// [warning] Please note that this function is not thread safe
    /// - Arguments
    ///     - index: The index of the bit, starting from 0
    ///     - value: The bit boolean value which will be save
    ///
    /// - Return
    ///     - Ok(true): Set and change the bit value
    ///     - Ok(false): The previous value is same as the current new value
    ///     - Err(KernelError::IndexOutOfRange): index out of range
    ///
    /// - Pnaic
    ///     - When you change the bit value asynchronously
    pub(crate) fn set_bit(&self, index: usize, value: bool) -> Result<bool> {
        let per_value = self.get_bit(index)?;
        if per_value == value {
            Ok(false)
        } else {
            let mut map = self.map.exclusive_access();
            let offset = Self::offset(index);
            if let Some(block) = map.get_mut(offset) {
                let old_value = block.clone();
                let new_value = if value {
                    old_value | 1u8 << Self::limit(index)
                } else {
                    old_value & (255u8 - (1u8 << Self::limit(index)))
                };
                *block = new_value;
                Ok(true)
            } else {
                Err(KernelError::IndexOutOfRange {
                    index,
                    start: 0,
                    end: self.length,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_bitmap_offset() {
        assert_eq!(BitMap::offset(0), 0);
        assert_eq!(BitMap::offset(1), 0);
        assert_eq!(BitMap::offset(7), 0);
        assert_eq!(BitMap::offset(8), 1);
        assert_eq!(BitMap::offset(9), 1);
        assert_eq!(BitMap::offset(15), 1);
        assert_eq!(BitMap::offset(16), 2);
    }

    #[test_case]
    fn test_bitmap_limit() {
        assert_eq!(BitMap::limit(0), 0);
        assert_eq!(BitMap::limit(1), 1);
        assert_eq!(BitMap::limit(7), 7);
        assert_eq!(BitMap::limit(8), 0);
        assert_eq!(BitMap::limit(9), 1);
        assert_eq!(BitMap::limit(15), 7);
        assert_eq!(BitMap::limit(16), 0);
    }

    #[test_case]
    fn test_bitmap_block_size() {
        assert_eq!(BitMap::new(0).block_size(), 0);
        assert_eq!(BitMap::new(1).block_size(), 1);
        assert_eq!(BitMap::new(7).block_size(), 1);
        assert_eq!(BitMap::new(8).block_size(), 1);
        assert_eq!(BitMap::new(9).block_size(), 2);
    }

    #[test_case]
    fn test_bitmap_get_and_set_bit() {
        let map = BitMap::new(0);
        assert!(map.get_bit(0).is_err());
        assert!(map.get_bit(1).is_err());
        assert!(map.get_bit(8).is_err());
        assert!(map.set_bit(0, false).is_err());
        assert!(map.set_bit(1, false).is_err());
        assert!(map.set_bit(8, false).is_err());

        let map = BitMap::new(8);
        assert!(map.get_bit(8).is_err());
        assert!(map.get_bit(9).is_err());
        assert!(map.get_bit(10).is_err());
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(1).unwrap(), false);
        assert_eq!(map.get_bit(7).unwrap(), false);

        let map = BitMap::new(12);
        assert!(map.get_bit(12).is_err());
        assert!(map.get_bit(13).is_err());
        assert!(map.get_bit(16).is_err());
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(8).unwrap(), false);
        assert_eq!(map.get_bit(11).unwrap(), false);

        let map = BitMap::new(3);
        assert!(map.get_bit(3).is_err());
        assert!(map.get_bit(7).is_err());
        assert!(map.get_bit(8).is_err());
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(1).unwrap(), false);
        assert_eq!(map.get_bit(2).unwrap(), false);

        assert!(map.set_bit(3, false).is_err());
        assert!(map.set_bit(7, false).is_err());
        assert!(map.set_bit(8, false).is_err());
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(1).unwrap(), false);
        assert_eq!(map.get_bit(2).unwrap(), false);

        assert_eq!(map.set_bit(0, false).unwrap(), false);
        assert_eq!(map.set_bit(1, false).unwrap(), false);
        assert_eq!(map.set_bit(2, false).unwrap(), false);
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(1).unwrap(), false);
        assert_eq!(map.get_bit(2).unwrap(), false);

        assert_eq!(map.set_bit(0, true).unwrap(), true);
        assert_eq!(map.set_bit(1, true).unwrap(), true);
        assert_eq!(map.set_bit(2, true).unwrap(), true);
        assert_eq!(map.get_bit(0).unwrap(), true);
        assert_eq!(map.get_bit(1).unwrap(), true);
        assert_eq!(map.get_bit(2).unwrap(), true);

        assert_eq!(map.set_bit(0, true).unwrap(), false);
        assert_eq!(map.set_bit(1, true).unwrap(), false);
        assert_eq!(map.set_bit(2, true).unwrap(), false);
        assert_eq!(map.get_bit(0).unwrap(), true);
        assert_eq!(map.get_bit(1).unwrap(), true);
        assert_eq!(map.get_bit(2).unwrap(), true);

        assert_eq!(map.set_bit(0, false).unwrap(), true);
        assert_eq!(map.set_bit(1, false).unwrap(), true);
        assert_eq!(map.set_bit(2, false).unwrap(), true);
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(1).unwrap(), false);
        assert_eq!(map.get_bit(2).unwrap(), false);

        assert_eq!(map.set_bit(0, false).unwrap(), false);
        assert_eq!(map.set_bit(1, true).unwrap(), true);
        assert_eq!(map.set_bit(2, false).unwrap(), false);
        assert_eq!(map.get_bit(0).unwrap(), false);
        assert_eq!(map.get_bit(1).unwrap(), true);
        assert_eq!(map.get_bit(2).unwrap(), false);
        assert_eq!(map.set_bit(0, true).unwrap(), true);
        assert_eq!(map.set_bit(1, false).unwrap(), true);
        assert_eq!(map.set_bit(2, false).unwrap(), false);
        assert_eq!(map.get_bit(0).unwrap(), true);
        assert_eq!(map.get_bit(1).unwrap(), false);
        assert_eq!(map.get_bit(2).unwrap(), false);
    }
}

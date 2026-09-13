use metafile_core::{MetafileError, Result};

#[derive(Clone, Copy)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Reader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base: 0,
        }
    }

    #[cfg(test)]
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    #[cfg(test)]
    pub fn i16(&mut self) -> Result<i16> {
        let bytes = self.take(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    #[cfg(test)]
    pub fn f32(&mut self) -> Result<f32> {
        let bytes = self.take(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn take(&mut self, amount: usize) -> Result<&'a [u8]> {
        let start = self.pos;
        let end = start
            .checked_add(amount)
            .ok_or_else(|| self.truncated(start, amount))?;
        if end > self.data.len() {
            return Err(self.truncated(start, amount));
        }
        self.pos = end;
        Ok(&self.data[start..end])
    }

    fn truncated(self, offset: usize, needed: usize) -> MetafileError {
        MetafileError::TruncatedInput {
            offset: self.base.saturating_add(offset),
            needed,
            available: self.data.len().saturating_sub(offset),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_emf_scalars_and_checks_bounds() {
        let mut reader = Reader::new(&[0xfe, 0xff, 0, 0, 0x80, 0x3f]);
        assert_eq!(reader.i16().unwrap(), -2);
        assert_eq!(reader.f32().unwrap(), 1.0);
        assert!(reader.u8().is_err());
    }
}

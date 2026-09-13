use metafile_core::{MetafileError, Result};

#[derive(Clone, Copy)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub const fn position(self) -> usize {
        self.pos
    }
    pub const fn remaining(self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(self.truncated(pos, 0));
        }
        self.pos = pos;
        Ok(())
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        let p = self
            .pos
            .checked_add(n)
            .ok_or_else(|| self.truncated(usize::MAX, n))?;
        self.seek(p)
    }
    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn i16(&mut self) -> Result<i16> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let start = self.pos;
        let end = start
            .checked_add(n)
            .ok_or_else(|| self.truncated(start, n))?;
        if end > self.data.len() {
            return Err(self.truncated(start, n));
        }
        self.pos = end;
        Ok(&self.data[start..end])
    }
    fn truncated(self, offset: usize, needed: usize) -> MetafileError {
        MetafileError::TruncatedInput {
            offset,
            needed,
            available: self.data.len().saturating_sub(offset),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_signed_little_endian() {
        let mut r = Reader::new(&[0x34, 0x12, 0xfe, 0xff]);
        assert_eq!(r.u16().unwrap(), 0x1234);
        assert_eq!(r.i16().unwrap(), -2);
    }
    #[test]
    fn bounds_checked() {
        let mut r = Reader::new(&[1]);
        assert!(matches!(r.u16(), Err(MetafileError::TruncatedInput { .. })));
    }
    #[test]
    fn seek_is_checked() {
        let mut r = Reader::new(&[]);
        assert!(r.seek(usize::MAX).is_err());
    }
}

use std::ops::Range;
/// Owns one bounded response buffer; blob access borrows slices without copying file bodies.
pub struct GitBlobBatch {
    pub(crate) bytes: Vec<u8>,
    pub(crate) ranges: Vec<Range<usize>>,
}
impl GitBlobBatch {
    pub fn len(&self) -> usize {
        self.ranges.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&[u8]> {
        self.ranges
            .get(index)
            .and_then(|range| self.bytes.get(range.clone()))
    }
}

use crate::{
    compiled_lexical_region::CompiledLexicalRegion, lexical_region_frame::LexicalRegionFrame,
};
pub(crate) struct LexicalRegionClosing;
impl LexicalRegionClosing {
    pub fn starts(rest: &str, expected: &str, insensitive: bool) -> bool {
        rest.get(..expected.len()).is_some_and(|prefix| {
            if insensitive {
                prefix.eq_ignore_ascii_case(expected)
            } else {
                prefix == expected
            }
        })
    }
    pub fn length(
        rest: &str,
        frame: &LexicalRegionFrame,
        spec: &CompiledLexicalRegion,
    ) -> Option<usize> {
        if frame.header {
            return None;
        }
        let d = &spec.definition;
        let mut length = None;
        if !d.close.is_empty() && Self::starts(rest, &d.close, d.close_ascii_case_insensitive) {
            let mut end = d.close.len();
            let value = frame.close_value.as_deref().unwrap_or("");
            if Self::starts(&rest[end..], value, d.close_ascii_case_insensitive) {
                end += value.len();
                if let Some(tail) = &spec.close_tail {
                    if let Some(m) = tail.find(&rest[end..]) {
                        length = Some(end + m.end());
                    }
                } else {
                    length = Some(end);
                }
            }
        }
        d.alternate_closes
            .iter()
            .filter(|s| Self::starts(rest, s, d.close_ascii_case_insensitive))
            .map(|s| s.len())
            .chain(length)
            .max()
    }
}

use crate::SourceReadOutcome;
use sha2::{Digest, Sha256};
pub struct SourceTextDecoder;
impl SourceTextDecoder {
    pub fn decode(bytes: &[u8]) -> SourceReadOutcome {
        let checksum = format!("{:x}", Sha256::digest(bytes));
        let (encoding, result) = if bytes.starts_with(&[0xff, 0xfe, 0, 0])
            || bytes.starts_with(&[0, 0, 0xfe, 0xff])
        {
            (
                "UTF-32",
                Err("unsupported UTF-32 BOM; use UTF-8 or BOM-marked UTF-16".into()),
            )
        } else if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
            let little = bytes[0] == 0xff;
            let text = if !(bytes.len() - 2).is_multiple_of(2) {
                Err("invalid UTF-16: incomplete code unit".into())
            } else {
                let units = bytes[2..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| {
                        if little {
                            u16::from_le_bytes([v[0], v[1]])
                        } else {
                            u16::from_be_bytes([v[0], v[1]])
                        }
                    })
                    .collect::<Vec<_>>();
                String::from_utf16(&units).map_err(|_| "invalid UTF-16: unpaired surrogate".into())
            };
            (if little { "UTF-16LE" } else { "UTF-16BE" }, text)
        } else {
            let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
            let offset = if bom { 3 } else { 0 };
            (
                if bom { "UTF-8-BOM" } else { "UTF-8" },
                std::str::from_utf8(&bytes[offset..])
                    .map(str::to_owned)
                    .map_err(|e| {
                        format!(
                            "invalid UTF-8 at byte {}; use UTF-8 or BOM-marked UTF-16",
                            e.valid_up_to() + offset
                        )
                    }),
            )
        };
        match result {
            Ok(content) => SourceReadOutcome::Decoded {
                content,
                encoding,
                checksum,
                raw_bytes: bytes.len(),
            },
            Err(reason) => SourceReadOutcome::Skipped {
                reason,
                checksum,
                raw_bytes: bytes.len(),
            },
        }
    }
}

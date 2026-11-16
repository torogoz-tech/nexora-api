use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{EvidenceUpload, PlatformError};
pub(crate) struct EvidenceValidation;
impl EvidenceValidation {
    pub fn decode(input: &EvidenceUpload) -> Result<Vec<u8>, PlatformError> {
        if input.repository_id.is_empty()
            || input.repository_id.len() > 128
            || input.name.trim().is_empty()
            || input.name.len() > 200
            || input.name.chars().any(char::is_control)
            || input.description.len() > 8000
            || !matches!(input.phase.as_str(), "before" | "after" | "supporting")
            || input.document_id.is_some() != input.document_version_id.is_some()
            || input
                .document_id
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 128)
            || input
                .document_version_id
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 128)
            || !(input.provided_metadata.is_null() || input.provided_metadata.is_object())
            || input.provided_metadata.to_string().len() > 8192
            || input.data_base64.len() > 5592408
        {
            return Err(PlatformError::Validation(
                "invalid evidence metadata or size",
            ));
        }
        let bytes = STANDARD
            .decode(&input.data_base64)
            .map_err(|_| PlatformError::Validation("invalid evidence base64"))?;
        Self::content(&input.media_type, &bytes)?;
        Ok(bytes)
    }
    pub fn content(media: &str, bytes: &[u8]) -> Result<(), PlatformError> {
        if bytes.is_empty() || bytes.len() > 4194304 {
            return Err(PlatformError::Validation(
                "evidence must contain1byte to4MiB",
            ));
        }
        let valid = match media {
            "image/png" => {
                bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    && bytes.len() >= 45
                    && &bytes[12..16] == b"IHDR"
                    && &bytes[bytes.len() - 8..bytes.len() - 4] == b"IEND"
            }
            "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff") && bytes.ends_with(b"\xff\xd9"),
            "image/webp" => {
                bytes.len() >= 20
                    && bytes.starts_with(b"RIFF")
                    && &bytes[8..12] == b"WEBP"
                    && u32::from_le_bytes(
                        bytes[4..8]
                            .try_into()
                            .map_err(|_| PlatformError::Validation("invalid WebP header"))?,
                    ) as usize
                        + 8
                        == bytes.len()
            }
            "text/plain" | "text/markdown" => std::str::from_utf8(bytes).is_ok_and(|s| {
                !s.chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            }),
            "application/json" => serde_json::from_slice::<serde_json::Value>(bytes).is_ok(),
            _ => false,
        };
        if !valid {
            return Err(PlatformError::Validation(
                "unsupported or invalid evidence format",
            ));
        }
        Ok(())
    }
}

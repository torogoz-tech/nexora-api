use nexora_platform::PlatformError;
pub(crate) struct KnowledgeFile;
impl KnowledgeFile {
    pub const MAX_BYTES: usize = 5 * 1024 * 1024;
    pub fn mime(name: &str, bytes: &[u8]) -> Result<&'static str, PlatformError> {
        if name.is_empty()
            || name.len() > 240
            || name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
            || bytes.is_empty()
            || bytes.len() > Self::MAX_BYTES
        {
            return Err(PlatformError::Validation("invalid document; maximum 5 MiB"));
        }
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let mime = match ext.as_str() {
            "pdf" if bytes.starts_with(b"%PDF-") => "application/pdf",
            "docx" if bytes.starts_with(b"PK") => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            "xlsx" if bytes.starts_with(b"PK") => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            "pptx" if bytes.starts_with(b"PK") => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            "md" | "txt" | "csv" if std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0) => {
                "text/plain"
            }
            _ => {
                return Err(PlatformError::Validation(
                    "supported documents: PDF, DOCX, XLSX, PPTX, TXT, Markdown and CSV",
                ));
            }
        };
        Ok(mime)
    }
}

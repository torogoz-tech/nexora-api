#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitObjectFormat {
    Sha1,
    Sha256,
}
impl GitObjectFormat {
    pub(crate) fn validate_oid(self, oid: &str) -> Result<(), crate::SourceError> {
        if oid.len() != self.oid_length()
            || !oid
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(crate::SourceError::InvalidObject);
        }
        Ok(())
    }
    pub fn oid_length(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha256 => 64,
        }
    }
}

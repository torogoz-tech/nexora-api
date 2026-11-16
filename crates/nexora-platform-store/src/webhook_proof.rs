use nexora_platform::PlatformError;
/// Trusted transport metadata, never a replacement for body signature validation.
pub struct WebhookProof {
    pub delivery_id: String,
    pub event: String,
    pub signature: String,
    pub timestamp: Option<String>,
}
impl WebhookProof {
    pub(crate) fn validate(&self) -> Result<(), PlatformError> {
        if self.delivery_id.is_empty()
            || self.delivery_id.len() > 128
            || !self
                .delivery_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-:".contains(&b))
            || self.event.is_empty()
            || self.event.len() > 64
            || !self
                .event
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_ -".contains(&b))
            || self.signature.len() > 2048
        {
            return Err(PlatformError::Unauthorized);
        }
        Ok(())
    }
}

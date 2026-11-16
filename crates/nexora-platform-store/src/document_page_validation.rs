use nexora_platform::PlatformError;
pub(crate) struct DocumentPageValidation;
impl DocumentPageValidation {
    pub fn limit(limit: u16) -> Result<(), PlatformError> {
        if !(1..=100).contains(&limit) {
            return Err(PlatformError::Validation(
                "document page limit must be1..100",
            ));
        }
        Ok(())
    }
    pub fn cursor(after: Option<&str>) -> Result<(), PlatformError> {
        if after.is_some_and(|value| {
            value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
        }) {
            return Err(PlatformError::Validation("invalid document page cursor"));
        }
        Ok(())
    }
}

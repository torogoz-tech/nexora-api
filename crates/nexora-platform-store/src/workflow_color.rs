use nexora_platform::PlatformError;
pub(crate) fn workflow_color(value: Option<&str>) -> Result<Option<String>, PlatformError> {
    match value {
        None | Some("") => Ok(None),
        Some(color)
            if color.len() == 7
                && color.starts_with('#')
                && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit) =>
        {
            Ok(Some(color.to_ascii_uppercase()))
        }
        _ => Err(PlatformError::Validation("invalid workflow color")),
    }
}

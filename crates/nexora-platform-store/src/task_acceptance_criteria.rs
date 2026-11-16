use nexora_platform::PlatformError;
pub(crate) fn task_acceptance_criteria(
    values: &[String],
) -> Result<serde_json::Value, PlatformError> {
    if values.len() > 100 || values.iter().map(String::len).sum::<usize>() > 32000 {
        return Err(PlatformError::Validation(
            "use up to 100 acceptance criteria and 32000 bytes in total",
        ));
    }
    let items = values
        .iter()
        .map(|value| value.trim().to_owned())
        .collect::<Vec<_>>();
    if items
        .iter()
        .any(|value| value.is_empty() || value.len() > 4000)
    {
        return Err(PlatformError::Validation(
            "each acceptance criterion must contain between 1 and 4000 bytes",
        ));
    }
    Ok(serde_json::json!(items))
}

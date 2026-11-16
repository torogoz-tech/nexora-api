use nexora_platform::PlatformError;
pub(crate) fn email_verification_code() -> Result<String, PlatformError> {
    let mut code = String::new();
    while code.len() < 6 {
        let mut byte = [0u8];
        getrandom::fill(&mut byte).map_err(|_| PlatformError::Unavailable)?;
        if byte[0] < 250 {
            code.push(char::from(b'0' + byte[0] % 10));
        }
    }
    Ok(code)
}

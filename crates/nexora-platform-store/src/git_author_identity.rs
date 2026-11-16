use nexora_platform::PlatformError;
pub(crate) struct GitAuthorIdentity;
impl GitAuthorIdentity {
    pub fn host(value: &str) -> Result<String, PlatformError> {
        let value = value.trim().to_ascii_lowercase();
        let url = url::Url::parse(&format!("https://{value}"))
            .map_err(|_| PlatformError::Validation("invalid Git author host"))?;
        if value.is_empty()
            || value.len() > 253
            || url.host_str().is_none()
            || url.host_str().is_some_and(|h| {
                value != h && value != format!("{h}:{}", url.port().unwrap_or(443))
            })
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(PlatformError::Validation("invalid Git author host"));
        }
        Ok(match url.port() {
            Some(port) => format!("{}:{port}", url.host_str().unwrap()),
            None => url.host_str().unwrap().to_string(),
        })
    }
    pub fn email(value: &str) -> Option<String> {
        let v = value.trim().to_lowercase();
        let parts: Vec<_> = v.split('@').collect();
        if v.len() > 320
            || parts.len() != 2
            || parts[0].is_empty()
            || parts[1].is_empty()
            || v.chars().any(|c| {
                c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '[' | ']')
            })
        {
            None
        } else {
            Some(v)
        }
    }
}

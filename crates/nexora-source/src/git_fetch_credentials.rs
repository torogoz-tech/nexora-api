use crate::SourceError;
use base64::{Engine, engine::general_purpose::STANDARD};
use zeroize::Zeroizing;
/// Transient provider credentials, deliberately neither Debug nor Serialize.
pub struct GitFetchCredentials {
    pub(crate) header: Zeroizing<String>,
}
impl GitFetchCredentials {
    pub fn new(username: &str, token: String) -> Result<Self, SourceError> {
        let token = Zeroizing::new(token);
        if !matches!(username, "oauth2" | "x-access-token")
            || token.is_empty()
            || token.len() > 8192
            || token.chars().any(char::is_control)
        {
            return Err(SourceError::InvalidConfiguration);
        }
        let clear = Zeroizing::new(format!("{username}:{}", token.as_str()));
        let encoded = Zeroizing::new(STANDARD.encode(clear.as_bytes()));
        Ok(Self {
            header: Zeroizing::new(format!("Authorization: Basic {}", encoded.as_str())),
        })
    }
}

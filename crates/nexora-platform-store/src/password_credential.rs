use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use nexora_platform::PlatformError;
pub(crate) struct PasswordCredential;
impl PasswordCredential {
    pub fn hash(password: &str) -> Result<String, PlatformError> {
        if password.len() < 12 || password.len() > 1024 {
            return Err(PlatformError::Validation(
                "password must contain 12..1024 bytes",
            ));
        }
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|p| p.to_string())
            .map_err(|_| PlatformError::Unavailable)
    }
    pub fn verify(password: &str, hash: &str) -> bool {
        password.len() <= 1024
            && PasswordHash::new(hash).is_ok_and(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
    }
}

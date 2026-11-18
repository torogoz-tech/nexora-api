use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use nexora_platform::PlatformError;
pub(crate) struct PasswordCredential;
impl PasswordCredential {
    fn algorithm() -> Result<Argon2<'static>, PlatformError> {
        let params = Params::new(19 * 1024, 2, 1, None).map_err(|_| PlatformError::Unavailable)?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
    pub fn hash(password: &str) -> Result<String, PlatformError> {
        if password.len() < 12 || password.len() > 1024 {
            return Err(PlatformError::Validation(
                "password must contain 12..1024 bytes",
            ));
        }
        Self::algorithm()?
            .hash_password(password.as_bytes())
            .map(|p| p.to_string())
            .map_err(|_| PlatformError::Unavailable)
    }
    pub fn verify(password: &str, hash: &str) -> bool {
        password.len() <= 1024
            && PasswordHash::new(hash).is_ok_and(|parsed| {
                Self::algorithm().is_ok_and(|argon2| {
                    argon2.verify_password(password.as_bytes(), &parsed).is_ok()
                })
            })
    }

    pub fn needs_rehash(hash: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(hash) else {
            return false;
        };
        let Ok(params) = Params::try_from(&parsed) else {
            return true;
        };
        parsed.algorithm.as_str() != "argon2id"
            || parsed.version != Some(Version::V0x13.into())
            || params.m_cost() < 19 * 1024
            || params.t_cost() < 2
            || params.p_cost() < 1
    }
}

#[cfg(test)]
mod tests {
    use super::PasswordCredential;
    use argon2::{Version, password_hash::phc::PasswordHash};

    #[test]
    fn password_hash_uses_explicit_argon2id_parameters() {
        let hash = PasswordCredential::hash("correct horse battery staple").unwrap();
        let parsed = PasswordHash::new(&hash).unwrap();

        assert_eq!(parsed.algorithm.as_str(), "argon2id");
        assert_eq!(parsed.version, Some(Version::V0x13.into()));
        assert_eq!(parsed.params.get_decimal("m"), Some(19 * 1024));
        assert_eq!(parsed.params.get_decimal("t"), Some(2));
        assert_eq!(parsed.params.get_decimal("p"), Some(1));
        assert!(PasswordCredential::verify(
            "correct horse battery staple",
            &hash
        ));
        assert!(!PasswordCredential::needs_rehash(&hash));
    }
}

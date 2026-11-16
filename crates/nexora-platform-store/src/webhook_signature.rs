use crate::WebhookProof;
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use nexora_platform::PlatformError;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
pub(crate) struct WebhookSignature;
impl WebhookSignature {
    pub fn github(secret: &str, proof: &WebhookProof, body: &[u8]) -> Result<(), PlatformError> {
        proof.validate()?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
            .map_err(|_| PlatformError::Storage)?;
        mac.update(body);
        let expected = format!(
            "sha256={}",
            mac.finalize()
                .into_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        if !bool::from(expected.as_bytes().ct_eq(proof.signature.as_bytes())) {
            return Err(PlatformError::Unauthorized);
        }
        Ok(())
    }
    pub fn key(token: &str) -> Result<Zeroizing<Vec<u8>>, PlatformError> {
        if token.len() > 128 {
            return Err(PlatformError::Validation("invalid GitLab signing token"));
        }
        let key = token
            .strip_prefix("whsec_")
            .ok_or(PlatformError::Validation(
                "expected GitLab whsec signing token",
            ))?;
        let decoded = Zeroizing::new(
            STANDARD
                .decode(key)
                .map_err(|_| PlatformError::Validation("invalid signing token encoding"))?,
        );
        if !(32..=64).contains(&decoded.len()) {
            return Err(PlatformError::Validation("invalid signing token size"));
        }
        Ok(decoded)
    }
    pub fn standard(key: &[u8], proof: &WebhookProof, body: &[u8]) -> Result<(), PlatformError> {
        proof.validate()?;
        let timestamp = proof
            .timestamp
            .as_deref()
            .filter(|v| v.len() <= 20)
            .ok_or(PlatformError::Unauthorized)?;
        let seconds = timestamp
            .parse::<i64>()
            .map_err(|_| PlatformError::Unauthorized)?;
        if seconds <= 0
            || time::OffsetDateTime::now_utc()
                .unix_timestamp()
                .abs_diff(seconds)
                > 300
        {
            return Err(PlatformError::Unauthorized);
        }
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(key).map_err(|_| PlatformError::Storage)?;
        mac.update(format!("{}.{}.", proof.delivery_id, timestamp).as_bytes());
        mac.update(body);
        let expected = format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()));
        if !proof
            .signature
            .split_whitespace()
            .take(20)
            .any(|v| bool::from(expected.as_bytes().ct_eq(v.as_bytes())))
        {
            return Err(PlatformError::Unauthorized);
        }
        Ok(())
    }
}

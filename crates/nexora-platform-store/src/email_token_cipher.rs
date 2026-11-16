use crate::{email_token_envelope::EmailTokenEnvelope, master_key::MasterKey};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use nexora_platform::PlatformError;
use serde_json::Value;
use zeroize::Zeroizing;
pub(crate) struct EmailTokenCipher {
    key: Zeroizing<[u8; 32]>,
    namespace: String,
}
impl EmailTokenCipher {
    pub fn new(master: MasterKey, namespace: &str) -> Result<Self, PlatformError> {
        Ok(Self {
            key: master.email_action_key(namespace)?,
            namespace: namespace.into(),
        })
    }
    fn aad(&self, id: &str, revision: i64) -> Vec<u8> {
        format!("nexora-email-token-v1:{}:{id}:{revision}", self.namespace).into_bytes()
    }
    pub fn seal(&self, id: &str, revision: i64, token: &str) -> Result<Value, PlatformError> {
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| PlatformError::Storage)?;
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).map_err(|_| PlatformError::Unavailable)?;
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: token.as_bytes(),
                    aad: &self.aad(id, revision),
                },
            )
            .map_err(|_| PlatformError::Storage)?;
        serde_json::to_value(EmailTokenEnvelope { nonce, ciphertext })
            .map_err(|_| PlatformError::Storage)
    }
    pub fn open(
        &self,
        id: &str,
        revision: i64,
        value: Value,
    ) -> Result<Zeroizing<String>, PlatformError> {
        let e: EmailTokenEnvelope =
            serde_json::from_value(value).map_err(|_| PlatformError::Storage)?;
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| PlatformError::Storage)?;
        let value = Zeroizing::new(
            cipher
                .decrypt(
                    &Nonce::from(e.nonce),
                    Payload {
                        msg: &e.ciphertext,
                        aad: &self.aad(id, revision),
                    },
                )
                .map_err(|_| PlatformError::Storage)?,
        );
        Ok(Zeroizing::new(
            String::from_utf8(value.to_vec()).map_err(|_| PlatformError::Storage)?,
        ))
    }
}

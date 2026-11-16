use serde::{Deserialize, Serialize};
use zeroize::Zeroize;
#[derive(Deserialize, Serialize)]
pub(crate) struct OidcPendingSecrets {
    pub verifier: String,
    pub nonce: String,
}
impl Drop for OidcPendingSecrets {
    fn drop(&mut self) {
        self.verifier.zeroize();
        self.nonce.zeroize();
    }
}

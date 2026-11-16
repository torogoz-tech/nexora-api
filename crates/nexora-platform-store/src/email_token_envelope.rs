use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
pub(crate) struct EmailTokenEnvelope {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

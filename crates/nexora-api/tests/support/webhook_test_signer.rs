use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use nexora_platform_store::WebhookProof;
use sha2::Sha256;
use std::{
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
pub struct WebhookTestSigner {
    key: Zeroizing<Vec<u8>>,
}
impl WebhookTestSigner {
    pub fn new() -> Self {
        let mut key = Zeroizing::new(vec![0u8; 32]);
        std::fs::File::open("/dev/urandom")
            .unwrap()
            .read_exact(&mut key)
            .unwrap();
        Self { key }
    }
    pub fn token(&self) -> String {
        format!("whsec_{}", STANDARD.encode(&*self.key))
    }
    pub fn proof(&self, id: &str, body: &[u8]) -> WebhookProof {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.key).unwrap();
        mac.update(format!("{id}.{timestamp}.").as_bytes());
        mac.update(body);
        WebhookProof {
            delivery_id: id.into(),
            event: "Push Hook".into(),
            signature: format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes())),
            timestamp: Some(timestamp),
        }
    }
}

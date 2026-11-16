use zeroize::Zeroizing;
pub struct ServiceRuntimeSettings {
    pub settings: serde_json::Value,
    pub secret: Option<Zeroizing<Vec<u8>>>,
}

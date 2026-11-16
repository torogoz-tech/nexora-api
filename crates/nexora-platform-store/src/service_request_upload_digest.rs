use sha2::{Digest, Sha256};
pub struct ServiceRequestUploadDigest;
impl ServiceRequestUploadDigest {
    pub fn of(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
}

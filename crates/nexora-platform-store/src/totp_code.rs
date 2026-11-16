use hmac::{Hmac, Mac};
use sha1::Sha1;
use subtle::ConstantTimeEq;
/// RFC 6238: six digits, 30 seconds, one adjacent time step of clock skew.
pub(crate) struct TotpCode;
impl TotpCode {
    pub fn encode(secret: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let mut out = String::new();
        let mut bits = 0u32;
        let mut count = 0;
        for b in secret {
            bits = (bits << 8) | u32::from(*b);
            count += 8;
            while count >= 5 {
                count -= 5;
                out.push(ALPHABET[((bits >> count) & 31) as usize] as char);
            }
        }
        if count > 0 {
            out.push(ALPHABET[((bits << (5 - count)) & 31) as usize] as char);
        }
        out
    }
    pub fn code(secret: &[u8], step: i64) -> String {
        let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC accepts every key length");
        mac.update(&(step as u64).to_be_bytes());
        let bytes = mac.finalize().into_bytes();
        let offset = (bytes[19] & 15) as usize;
        let value = u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("four bytes"))
            & 0x7fffffff;
        format!("{:06}", value % 1_000_000)
    }
    pub fn verify(secret: &[u8], code: &str, now: i64, previous: i64) -> Option<i64> {
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        ((now / 30 - 1)..=(now / 30 + 1))
            .filter(|s| *s > previous)
            .find(|s| bool::from(Self::code(secret, *s).as_bytes().ct_eq(code.as_bytes())))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc_vectors_and_replay() {
        let key = b"12345678901234567890";
        assert_eq!(TotpCode::encode(key), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(TotpCode::code(key, 1), "287082");
        assert_eq!(TotpCode::code(key, 37037036), "081804");
        assert_eq!(TotpCode::verify(key, "287082", 59, -1), Some(1));
        assert_eq!(TotpCode::verify(key, "287082", 59, 1), None);
        assert_eq!(TotpCode::verify(key, "287082", 180, -1), None);
    }
}

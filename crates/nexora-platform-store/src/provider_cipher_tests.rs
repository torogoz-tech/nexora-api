use crate::{
    SecretGenerator, master_key::MasterKey, provider_secret_cipher::ProviderSecretCipher,
    provider_secret_context::ProviderSecretContext,
};
#[test]
fn provider_secrets_roundtrip_after_key_reload_and_reject_record_substitution_or_tampering() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(SecretGenerator::id("nexora-provider-cipher").unwrap());
    std::fs::create_dir(&root).unwrap();
    let namespace = "nexora_platform_cipher";
    let path = MasterKey::path(&root, namespace);
    let master = MasterKey::create_or_load(&path).unwrap();
    let context = ProviderSecretContext {
        binding_id: "conn_fixture".into(),
        provider: nexora_platform::GitProvider::Github.into(),
        host: "github.com".into(),
        purpose: "access_token".into(),
        revision: 1,
    };
    let secret = zeroize::Zeroizing::new(SecretGenerator::token("provider_test").unwrap());
    let cipher = ProviderSecretCipher::load(&root, namespace).unwrap();
    let envelope = cipher.seal(&context, secret.as_bytes()).unwrap();
    let again = cipher.seal(&context, secret.as_bytes()).unwrap();
    assert_ne!(envelope.nonce, again.nonce);
    assert!(envelope.ciphertext != again.ciphertext);
    assert!(
        !envelope
            .ciphertext
            .windows(secret.len())
            .any(|w| w == secret.as_bytes())
    );
    drop(master);
    let master = MasterKey::load(&path).unwrap();
    let cipher = ProviderSecretCipher::new(master, namespace);
    let plain = cipher.open(&context, &envelope).unwrap();
    assert!(plain.as_slice() == secret.as_bytes());
    let mut wrong = context.clone();
    wrong.binding_id = "conn_other".into();
    assert!(cipher.open(&wrong, &envelope).is_err());
    let mut wrong = context.clone();
    wrong.provider = nexora_platform::GitProvider::Gitlab.into();
    assert!(cipher.open(&wrong, &envelope).is_err());
    let mut wrong = context.clone();
    wrong.host = "gitlab.com".into();
    assert!(cipher.open(&wrong, &envelope).is_err());
    let mut wrong = context.clone();
    wrong.purpose = "refresh_token".into();
    assert!(cipher.open(&wrong, &envelope).is_err());
    let mut wrong = context.clone();
    wrong.revision = 2;
    assert!(cipher.open(&wrong, &envelope).is_err());
    assert!(
        ProviderSecretCipher::new(MasterKey::load(&path).unwrap(), "nexora_platform_other")
            .open(&context, &envelope)
            .is_err()
    );
    let mut tampered = envelope.clone();
    tampered.ciphertext[0] ^= 1;
    assert!(cipher.open(&context, &tampered).is_err());
    let mut tampered = envelope.clone();
    tampered.nonce[0] ^= 1;
    assert!(cipher.open(&context, &tampered).is_err());
    let mut tampered = envelope.clone();
    tampered.key_version = 2;
    assert!(cipher.open(&context, &tampered).is_err());
    let mut truncated = envelope.clone();
    truncated.ciphertext.truncate(10);
    assert!(cipher.open(&context, &truncated).is_err());
    assert!(cipher.seal(&context, b"").is_err());
    assert!(cipher.seal(&context, &vec![1; 65_537]).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

mod support;
use nexora_governance::*;
use std::{fs, path::Path};
#[test]
fn strict_yaml_rejects_unknown_duplicate_null_tags_and_resource_excess() {
    let original = fs::read_to_string("../../policies/examples/naming.policy.yaml").unwrap();
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-gov-yaml-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rule.yaml");
    for text in [
        format!("{original}\nunknown: true\n"),
        format!("{original}\nkind: Policy\n"),
        format!("{original}\n---\nkind: Policy\n"),
        original.replace("language: javascript", "language: null"),
        original.replace("onUnknown: review", "onUnknown: !tag review"),
        "x: [".into(),
        "x: ".to_owned() + &"[".repeat(30) + "0" + &"]".repeat(30),
        "x: ".to_owned() + &"a".repeat(65_537),
    ] {
        fs::write(&path, text).unwrap();
        assert!(GovernanceIo::yaml::<Policy>(&path).is_err());
    }
    fs::write(&path, &original).unwrap();
    assert!(PolicyRegistry::load(&dir).is_ok());
    fs::write(dir.join("duplicate.yaml"), original).unwrap();
    assert!(PolicyRegistry::load(&dir).is_err());
    assert!(GovernanceIo::path(&dir.join("../outside")).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, dir.join("link.yaml")).unwrap();
        assert!(GovernanceIo::yaml::<Policy>(&dir.join("link.yaml")).is_err());
    }
    fs::remove_dir_all(dir).unwrap();
    let schema: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(Path::new("../../schemas/policy-v1alpha1.schema.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        schema["properties"]["apiVersion"]["const"],
        "nexora.dev/v1alpha1"
    );
}
#[test]
fn versioned_cross_package_references_resolve_and_cycles_fail() {
    let mut a = support::policy();
    a.metadata.package = Some("base".into());
    let mut b = a.clone();
    b.metadata.package = Some("consumer".into());
    b.spec.requires = vec![PolicyReference {
        id: a.metadata.qualified_id(),
        version: a.metadata.version.clone(),
    }];
    let mut registry = PolicyRegistry::default();
    registry.register(a.clone()).unwrap();
    registry.register(b.clone()).unwrap();
    assert!(registry.validate_references().is_ok());
    a.spec.requires = vec![PolicyReference {
        id: b.metadata.qualified_id(),
        version: b.metadata.version.clone(),
    }];
    let mut registry = PolicyRegistry::default();
    registry.register(a).unwrap();
    registry.register(b).unwrap();
    assert!(registry.validate_references().is_err());
}

#[test]
fn typed_policies_roundtrip_through_the_strict_yaml_loader() {
    let policy = support::policy();
    let path = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-gov-roundtrip-{}.yaml", std::process::id()));
    fs::write(&path, serde_yaml_ng::to_string(&policy).unwrap()).unwrap();
    let loaded = GovernanceIo::yaml::<Policy>(&path);
    fs::remove_file(path).unwrap();
    assert!(loaded.is_ok(), "{loaded:?}");
}

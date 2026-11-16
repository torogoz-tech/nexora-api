use nexora_language::{AnalysisProfilePackage, LanguageDefinition};
fn package() -> AnalysisProfilePackage {
    AnalysisProfilePackage {
        format: "nexora.analysis-profile".into(),
        schema_version: 1,
        key: "lua-standard".into(),
        version: "1.0.0".into(),
        name: "Lua standard".into(),
        language_id: "lua".into(),
        models: vec![
            LanguageDefinition::load(std::path::Path::new("../../languages/lua")).unwrap(),
        ],
    }
}
#[test]
fn profile_roundtrip_preserves_fingerprint() {
    let p = package();
    p.validate().unwrap();
    let json = serde_json::to_vec(&p).unwrap();
    let restored: AnalysisProfilePackage = serde_json::from_slice(&json).unwrap();
    assert_eq!(p.checksum().unwrap(), restored.checksum().unwrap());
}
#[test]
fn profile_rejects_duplicate_models_and_unknown_paths() {
    let mut p = package();
    p.models.push(p.models[0].clone());
    assert!(p.validate().is_err());
    let mut value = serde_json::to_value(package()).unwrap();
    value["directory"] = serde_json::json!("/etc/nexora");
    assert!(serde_json::from_value::<AnalysisProfilePackage>(value).is_err());
}
#[test]
fn profile_rejects_missing_root_and_invalid_format() {
    let mut p = package();
    p.language_id = "missing".into();
    assert!(p.validate().is_err());
    p = package();
    p.schema_version = 2;
    assert!(p.validate().is_err());
}
#[test]
fn composition_roundtrip_includes_exact_dependencies_and_stable_order() {
    let p = AnalysisProfilePackage::from_catalog(
        std::path::Path::new("../../templates/composition-markup"),
        "region_carrier".into(),
        "composition".into(),
        "1.0.0".into(),
        "Composed profile".into(),
    )
    .unwrap();
    assert_eq!(p.models.len(), 2);
    let mut reverse = p.clone();
    reverse.models.reverse();
    assert_eq!(p.checksum().unwrap(), reverse.checksum().unwrap());
    reverse.models.pop();
    assert!(reverse.validate().is_err());
}
#[test]
fn numeric_package_version_rejects_signs() {
    let mut p = package();
    p.version = "+1.2.3".into();
    assert!(p.validate().is_err());
}

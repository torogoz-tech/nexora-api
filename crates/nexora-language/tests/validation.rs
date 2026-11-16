use nexora_language::LanguageDefinition;
use nexora_language::LanguageRegistry;
use std::path::Path;
#[test]
fn load_and_detect_reference_language() {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let mut registry = LanguageRegistry::default();
    registry.register(language).unwrap();
    assert_eq!(
        registry.detect(Path::new("app.js")).unwrap().manifest.id.0,
        "javascript"
    );
    assert!(registry.detect(Path::new("app.rs")).is_none());
}
#[test]
fn invalid_rules_are_rejected() {
    let mut language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    language.manifest.schema_version = 99;
    assert!(language.validate().is_err());
    language.manifest.schema_version = 1;
    language.lexer.tokens[0].pattern = "a*".into();
    assert!(language.validate().is_err());
}

#[test]
fn shared_extensions_require_explicit_selection_not_native_language_priority() {
    let first =
        LanguageDefinition::load(std::path::Path::new("../../languages/javascript")).unwrap();
    let mut other = first.clone();
    other.manifest.id = nexora_core::LanguageId("alternate-profile".into());
    let mut registry = LanguageRegistry::default();
    registry.register(first).unwrap();
    registry.register(other).unwrap();
    assert_eq!(
        registry.candidates(std::path::Path::new("file.js")).len(),
        2
    );
    assert!(registry.detect(std::path::Path::new("file.js")).is_none());
    assert!(registry.get("alternate-profile").is_some());
}

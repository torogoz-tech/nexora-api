use nexora_language::LanguageDefinition;
use std::{fs, path::Path};

#[test]
fn malformed_unknown_duplicate_and_multidocument_yaml_are_rejected() {
    let base = Path::new("../../languages/javascript");
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-yaml-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for name in ["language.yaml", "lexer.yaml", "grammar.yaml", "graph.yaml"] {
        fs::copy(base.join(name), dir.join(name)).unwrap();
    }
    let manifest = fs::read_to_string(base.join("language.yaml")).unwrap();
    for bad in [
        format!("{manifest}\nunknown_option: true\n"),
        format!("{manifest}\nid: duplicate\n"),
        format!("{manifest}\n---\nid: second\n"),
        "schema_version: [".into(),
    ] {
        fs::write(dir.join("language.yaml"), &bad).unwrap();
        assert!(LanguageDefinition::load(&dir).is_err(), "accepted {bad}");
    }
    fs::write(dir.join("language.yaml"), manifest).unwrap();
    fs::write(
        dir.join("graph.yaml"),
        "schema_version: 1\nmappings:\n  function: Function\n  function: Class\n  class: Class\n",
    )
    .unwrap();
    assert!(
        LanguageDefinition::load(&dir).is_err(),
        "duplicate extraction mapping accepted"
    );
    fs::remove_dir_all(dir).unwrap();
}

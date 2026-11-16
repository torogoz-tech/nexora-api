use nexora_corpus::{CorpusInput, CorpusMode, CorpusRunner, CorpusSuite};
use std::path::{Path, PathBuf};
fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-corpus-loader-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("pack")).unwrap();
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["language.yaml", "lexer.yaml", "grammar.yaml", "graph.yaml"] {
        std::fs::copy(
            workspace.join("templates/language-pack").join(name),
            root.join("pack").join(name),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("repo/source.nxexample"),
        "DEF total [] BEGIN FINISH\n",
    )
    .unwrap();
    std::fs::write(root.join("suite.yaml"),r#"{"schema_version":1,"id":"test","version":"1.0.0","packs":[{"id":"template","path":"pack"}],"features":[{"id":"test","description":"test"}],"cases":["case.yaml"]}"#).unwrap();
    std::fs::write(root.join("case.yaml"),r#"{"id":"test","pack":"template","feature":"test","classification":"supported","mode":"repository","input":"repo","replicas":2,"expected":{"analysis":"clean","facts":[{"type":"declaration","file":"source.nxexample","kind":"Function","name":"total","source":"DEF total [] BEGIN FINISH"}]}}"#).unwrap();
    root
}
#[test]
fn ignores_binary_non_sources_and_confines_symlinks() {
    let root = fixture("binary");
    std::fs::write(root.join("repo/image.bin"), [0xff, 0x00, 0xfe]).unwrap();
    let suite = CorpusSuite::load(Path::new("suite.yaml"), &root).unwrap();
    assert!(CorpusRunner::run(&suite, None, false).unwrap().passed);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            root.join("repo/source.nxexample"),
            root.join("repo/link.nxexample"),
        )
        .unwrap();
        assert!(CorpusSuite::load(Path::new("suite.yaml"), &root).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn rejects_expanded_directories_and_depth_before_materialization() {
    let root = fixture("entries");
    for i in 0..10 {
        std::fs::write(
            root.join(format!("repo/{i}.nxexample")),
            "DEF total [] BEGIN FINISH\n",
        )
        .unwrap();
    }
    let path = root.join("case.yaml");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("\"replicas\":2", "\"replicas\":1000")).unwrap();
    assert!(CorpusSuite::load(Path::new("suite.yaml"), &root).is_err());
    let deep = root.join("repo");
    let relative = (0..64).map(|_| "d").collect::<Vec<_>>().join("/");
    assert!(
        CorpusInput::validate_expansion(&deep, &[deep.join(relative).join("file.nxexample")], 2)
            .is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn rejects_unknown_fields_duplicate_yaml_and_unsupported_extensions() {
    let root = fixture("invalid");
    let path = root.join("case.yaml");
    let original = std::fs::read_to_string(&path).unwrap();
    for text in [
        original.replace("\"id\":\"test\"", "\"id\":\"test\",\"unexpected\":true"),
        "id: first\nid: duplicate\n".into(),
        original.replace("\"replicas\":2", "\"replicas\":0"),
    ] {
        std::fs::write(&path, text).unwrap();
        assert!(CorpusSuite::load(Path::new("suite.yaml"), &root).is_err());
    }
    let language = nexora_language::LanguageDefinition::load(&root.join("pack")).unwrap();
    assert!(CorpusInput::files(&root.join("repo/image.bin"), CorpusMode::File, &language).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

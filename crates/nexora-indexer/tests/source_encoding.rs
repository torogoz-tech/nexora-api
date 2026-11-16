use nexora_indexer::SourceReader;
use std::fs;
fn fixture(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-encoding-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    path
}
#[test]
fn decodes_bom_marked_utf8_and_utf16_without_replacement_characters() {
    let source = "public class Café {}\r\n";
    let mut utf8 = vec![0xef, 0xbb, 0xbf];
    utf8.extend(source.as_bytes());
    assert_eq!(
        SourceReader::read(&fixture("utf8.cs", &utf8)).unwrap(),
        source
    );
    for (name, bom, little) in [
        ("le.cs", [0xff, 0xfe], true),
        ("be.cs", [0xfe, 0xff], false),
    ] {
        let mut bytes = bom.to_vec();
        for unit in source.encode_utf16() {
            bytes.extend(if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        assert_eq!(SourceReader::read(&fixture(name, &bytes)).unwrap(), source);
    }
}
#[test]
fn rejects_unknown_encoding_and_malformed_bom_data_without_guessing() {
    for (name, bytes) in [
        ("legacy.cs", vec![b'/', b'/', 0xe9]),
        ("broken16.cs", vec![0xff, 0xfe, 0x00, 0xd8]),
        ("odd16.cs", vec![0xff, 0xfe, 0x61]),
        ("utf32.cs", vec![0xff, 0xfe, 0, 0, 0x61, 0, 0, 0]),
    ] {
        assert!(
            SourceReader::read(&fixture(name, &bytes)).is_err(),
            "{name}"
        );
    }
}
#[test]
fn scalar_indexing_preserves_the_skipped_path_and_original_byte_identity() {
    use nexora_indexer::{Indexer, MemoryStore};
    let good = fixture("legacy-scalar/good.nxexample", b"DEF good [] BEGIN FINISH");
    let root = good.parent().unwrap();
    fs::write(root.join("bad.nxexample"), [0xe9]).unwrap();
    let indexer = Indexer::new(
        nexora_language::LanguageDefinition::load(std::path::Path::new(
            "../../templates/language-pack",
        ))
        .unwrap(),
    );
    let snapshot = indexer
        .index(
            root,
            nexora_core::TenantId("encoding".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    assert!(snapshot.sources.contains_key("good.nxexample"));
    assert!(!snapshot.sources.contains_key("bad.nxexample"));
    assert!(
        snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "source_encoding" && d.path.as_deref() == Some("bad.nxexample"))
    );
    fs::write(root.join("bad.nxexample"), [0xe8]).unwrap();
    let changed = indexer
        .index(
            root,
            nexora_core::TenantId("encoding".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    assert_ne!(snapshot.id, changed.id);
}

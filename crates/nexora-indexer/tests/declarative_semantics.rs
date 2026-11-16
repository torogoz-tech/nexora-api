use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
fn analyze(
    pack: &str,
    extension: &str,
    source: &str,
    name: &str,
) -> nexora_indexer::AnalysisResult {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-declarative-{}-{name}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("source.{extension}"));
    fs::write(&path, source).unwrap();
    let result = Indexer::new(LanguageDefinition::load(Path::new(pack)).unwrap())
        .analyze(&path)
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    result
}
#[test]
fn exports_are_symbol_edges_with_their_original_evidence() {
    let result = analyze(
        "../../languages/javascript",
        "js",
        "export default identity;\nfunction identity(value) { return value; }\nexport { identity };\n",
        "exports",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let exports: Vec<_> = result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Exports)
        .collect();
    assert_eq!(exports.len(), 2);
    assert!(
        exports
            .iter()
            .all(|e| e.resolution == ResolutionState::Resolved)
    );
    assert_eq!(exports[0].label.as_deref(), Some("default"));
    assert_eq!(exports[1].label, None);
    assert_eq!(exports[0].target, exports[1].target);
}
#[test]
fn qualified_methods_and_namespaces_are_mapped_from_rules_only() {
    let result = analyze(
        "../../languages/lua",
        "lua",
        "-- function Fake:method() end\nfunction Object:new(value) function nested() end return value end\nfunction package.tools.run() end\n",
        "methods",
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let declarations: Vec<_> = result
        .graph
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Method | NodeKind::Function))
        .map(|n| (n.name.as_str(), n.kind.clone()))
        .collect();
    assert_eq!(
        declarations,
        vec![
            ("Object:new", NodeKind::Method),
            ("package.tools.run", NodeKind::Function)
        ]
    );
    let different = analyze(
        "../../templates/language-pack",
        "nxexample",
        "METHOD Receiver.run [] BEGIN FINISH\nPUBLISH Receiver.run;\n",
        "different",
    );
    assert!(
        different.diagnostics.is_empty(),
        "{:?}",
        different.diagnostics
    );
    assert!(
        different
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Method && n.name == "Receiver.run")
    );
    assert!(
        different
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Exports
                && e.resolution == ResolutionState::Resolved
                && e.label.as_deref() == Some("public"))
    );
}
#[test]
fn unsupported_exports_and_expressions_keep_diagnostics() {
    for (i, source) in [
        "export { x as alias };",
        "export { x, y };",
        "export { x } from './module.js';",
        "export default x();",
        "export default function x() {}",
    ]
    .iter()
    .enumerate()
    {
        let result = analyze(
            "../../languages/javascript",
            "js",
            source,
            &format!("unsupported-{i}"),
        );
        assert!(!result.diagnostics.is_empty(), "{source}");
        assert!(
            !result
                .graph
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::Exports && e.resolution == ResolutionState::Resolved)
        );
    }
}
#[test]
fn local_exports_never_resolve_from_another_file_and_incremental_matches_full() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, MemoryStore};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-declarative-cache-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("source.js"), "export default target;\n").unwrap();
    fs::write(root.join("other.js"), "function target() {}\n").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let first = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    assert!(
        first
            .snapshot
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Exports && e.resolution == ResolutionState::Unresolved)
    );
    fs::write(
        root.join("source.js"),
        "function target() {}\nexport default target;\n",
    )
    .unwrap();
    for _ in 0..2 {
        let result = indexer
            .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
            .unwrap();
        assert!(result.snapshot.diagnostics.is_empty());
        assert!(
            result
                .snapshot
                .graph
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::Exports
                    && e.resolution == ResolutionState::Resolved
                    && e.label.as_deref() == Some("default"))
        );
        let full = indexer
            .index(&root, TenantId("test".into()), &mut MemoryStore::default())
            .unwrap();
        assert_eq!(
            serde_json::to_value(&full).unwrap(),
            serde_json::to_value(&result.snapshot).unwrap()
        );
    }
    fs::write(
        root.join("source.js"),
        "function target() {}\nfunction target() {}\nexport default target;\n",
    )
    .unwrap();
    let changed = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.reused_files, 1);
    assert!(
        changed
            .snapshot
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Exports && e.resolution == ResolutionState::Ambiguous)
    );
    let full = indexer
        .index(&root, TenantId("test".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&changed.snapshot).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

use nexora_core::{SyntaxRole, TenantId};
use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{collections::HashSet, path::Path};
fn index(case: &str) -> nexora_storage::Snapshot {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    Indexer::new(language)
        .index(
            &Path::new("../../policies/architecture/domain-boundary/fixtures").join(case),
            TenantId("local".into()),
            &mut MemoryStore::default(),
        )
        .unwrap()
}
#[test]
fn references_resolve_to_real_files_with_exact_import_evidence() {
    let snapshot = index("fail");
    let edges: Vec<_> = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Imports | EdgeKind::DependsOn))
        .collect();
    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|e| e.resolution == ResolutionState::Resolved)
    );
    for edge in edges {
        let target = snapshot
            .graph
            .nodes
            .iter()
            .find(|n| n.id == edge.target)
            .unwrap();
        assert_eq!(target.kind, NodeKind::File);
        assert_eq!(target.name, "infrastructure/storage.js");
        let span = edge.evidence.span;
        assert_eq!(
            &snapshot.sources[&edge.evidence.path][span.start.offset..span.end.offset],
            "import '../infrastructure/storage.js';"
        );
    }
    assert!(
        !snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Module)
    );
    assert!(snapshot.diagnostics.is_empty());
    assert!(
        snapshot
            .source_coverage
            .iter()
            .filter(|c| c.path == "domain/order.js")
            .all(|c| c.complete)
    );
}
#[test]
fn unresolved_external_missing_and_ambiguous_modules_preserve_reference_nodes() {
    for (case, state) in [
        ("external", ResolutionState::Unresolved),
        ("missing", ResolutionState::Unresolved),
        ("ambiguous", ResolutionState::Ambiguous),
    ] {
        let snapshot = index(case);
        assert!(!snapshot.diagnostics.is_empty());
        assert!(
            snapshot
                .graph
                .edges
                .iter()
                .filter(|e| matches!(e.kind, EdgeKind::Imports | EdgeKind::DependsOn))
                .all(|e| e.resolution == state)
        );
        assert!(
            snapshot
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::Module)
        );
        assert!(
            snapshot
                .source_coverage
                .iter()
                .filter(|c| c.path == "domain/order.js")
                .all(|c| !c.complete)
        );
    }
}
#[test]
fn import_bindings_do_not_become_declared_symbols() {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let result = Indexer::new(language)
        .analyze(Path::new(
            "../../policies/architecture/domain-boundary/fixtures/fail/domain/order.js",
        ))
        .unwrap();
    assert_eq!(
        result
            .ast
            .nodes
            .iter()
            .filter(|n| n.role == SyntaxRole::Reference)
            .count(),
        1
    );
    assert_eq!(
        result
            .symbols
            .symbols
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Order"]
    );
    let ids: HashSet<_> = result.graph.nodes.iter().map(|n| &n.id).collect();
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| ids.contains(&e.source) && ids.contains(&e.target))
    );
}

mod support;
use support::temporary_repository::TemporaryRepository;
fn temporary(files: &[(&str, &str)]) -> nexora_storage::Snapshot {
    let repo = TemporaryRepository::new(files);
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    Indexer::new(language)
        .index(
            &repo.path,
            TenantId("local".into()),
            &mut MemoryStore::default(),
        )
        .unwrap()
}
#[test]
fn extension_directory_index_aliases_and_cycles_resolve_without_reloading_files() {
    let snapshot = temporary(&[
        (
            "domain/a.js",
            "import Default, {x as y} from '../shared/tool';\nimport * as Ns from '../shared/folder';\nclass A {}",
        ),
        ("shared/tool.js", "class Tool {}"),
        ("shared/folder/index.mjs", "class Folder {}"),
    ]);
    assert!(
        snapshot.diagnostics.is_empty(),
        "{:?}",
        snapshot.diagnostics
    );
    assert_eq!(
        snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports && e.resolution == ResolutionState::Resolved)
            .count(),
        2
    );
    let snapshot = temporary(&[
        ("domain/a.js", "import './b.js'; class A {}"),
        ("domain/b.js", "import './a.js'; class B {}"),
    ]);
    assert!(snapshot.diagnostics.is_empty());
    assert_eq!(
        snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::DependsOn && e.resolution == ResolutionState::Resolved)
            .count(),
        2
    );
}
#[test]
fn escaping_root_absolute_query_and_unsupported_literal_escapes_remain_unresolved() {
    for source in [
        "import '../../outside.js';",
        "import '/etc/passwd';",
        "import '../shared/tool.js?raw';",
        r"import '../shared/\u0074ool.js';",
    ] {
        let snapshot = temporary(&[
            ("domain/a.js", source),
            ("outside.js", "class Outside {}"),
            ("shared/tool.js", "class Tool {}"),
        ]);
        assert!(!snapshot.diagnostics.is_empty(), "{source}");
        assert!(
            snapshot
                .graph
                .edges
                .iter()
                .filter(|e| matches!(e.kind, EdgeKind::Imports | EdgeKind::DependsOn))
                .all(|e| e.resolution == ResolutionState::Unresolved)
        );
        assert!(
            snapshot
                .source_coverage
                .iter()
                .filter(|c| c.path == "domain/a.js")
                .all(|c| !c.complete)
        );
    }
}
#[test]
fn file_and_directory_candidates_do_not_silently_choose_a_winner() {
    let snapshot = temporary(&[
        ("domain/a.js", "import '../shared/tool';"),
        ("shared/tool.js", "class Tool {}"),
        ("shared/tool/index.js", "class Other {}"),
    ]);
    assert!(
        snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Imports)
            .all(|e| e.resolution == ResolutionState::Ambiguous)
    );
}

#[test]
fn candidate_templates_do_not_reinterpret_inserted_module_text() {
    let snapshot = temporary(&[
        ("main.js", "import './mod{extension}.js';"),
        ("mod{extension}.js", "function wanted() {}"),
        ("mod.js", "function wrong() {}"),
    ]);
    assert!(snapshot.diagnostics.is_empty());
    for edge in snapshot
        .graph
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Imports | EdgeKind::DependsOn))
    {
        let target = snapshot
            .graph
            .nodes
            .iter()
            .find(|n| n.id == edge.target)
            .unwrap();
        assert_eq!(target.name, "mod{extension}.js");
    }
}

use nexora_core::{RepositoryId, TenantId};
use nexora_graph::NodeKind;
use nexora_indexer::{
    AnalysisProfileCandidate, ExternalFileCache, MemoryStore, MultiProfileIndexer,
};
use nexora_language::AnalysisProfilePackage;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn candidate(catalog: &str, language: &str, id: &str, version: &str) -> AnalysisProfileCandidate {
    let package = AnalysisProfilePackage::from_catalog(
        Path::new(catalog),
        language.into(),
        format!("test-{language}"),
        version.into(),
        language.into(),
    )
    .unwrap();
    AnalysisProfileCandidate {
        id: id.into(),
        checksum: package.checksum().unwrap(),
        created_at: "2026-11-08T01:00:00.000000Z".into(),
        package,
    }
}
fn directory(name: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-multi-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}
fn run(indexer: &MultiProfileIndexer, root: &Path) -> nexora_indexer::MultiProfileIndexResult {
    let mut read = |_: &nexora_indexer::FileCacheKey| Ok(None);
    let mut write = |_: &nexora_indexer::FileCacheKey, _: &nexora_indexer::CacheFragment| Ok(false);
    indexer
        .index_repository_external(
            root,
            TenantId("tenant".into()),
            RepositoryId("repo_mixed".into()),
            &mut MemoryStore::default(),
            &mut ExternalFileCache {
                read: &mut read,
                write: &mut write,
            },
        )
        .unwrap()
}
#[test]
fn mixed_repository_has_one_snapshot_one_root_and_each_source_once() {
    let root = directory("mixed");
    for (path, source) in [
        ("A.cs", "public class A { public int Value { get; set; } }"),
        ("Index.razor", "<p>Hello</p>"),
        ("app.js", "function hello() {}"),
        ("app.css", ".hello { color: red; }"),
        ("README.md", "unknown"),
    ] {
        fs::write(root.join(path), source).unwrap();
    }
    fs::create_dir(root.join("node_modules")).unwrap();
    fs::write(root.join("node_modules/vendor.js"), "invalid").unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![
            candidate(
                "../../templates/dotnet-understanding",
                "csharp",
                "cs",
                "0.9.0",
            ),
            candidate(
                "../../templates/dotnet-understanding",
                "razor",
                "razor",
                "0.10.0",
            ),
            candidate("../../languages", "javascript", "js", "0.4.0"),
            candidate("../../templates/web-documents", "css", "css", "0.3.0"),
        ],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert_eq!(result.index.snapshot.sources.len(), 4);
    assert_eq!(result.index.reuse.read_files, 4);
    assert_eq!(
        result
            .index
            .snapshot
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Repository)
            .count(),
        1
    );
    assert_eq!(
        result
            .index
            .snapshot
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::File)
            .count(),
        4
    );
    assert_eq!(result.selection["profiles"].as_array().unwrap().len(), 4);
    assert_eq!(
        result.selection["uncovered_files"],
        serde_json::json!(["README.md"])
    );
    assert_eq!(
        result.selection["excluded_directories"],
        serde_json::json!(["node_modules/"])
    );
    let node_ids: std::collections::BTreeSet<_> = result
        .index
        .snapshot
        .graph
        .nodes
        .iter()
        .map(|n| &n.id)
        .collect();
    assert_eq!(node_ids.len(), result.index.snapshot.graph.nodes.len());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn latest_is_numeric_then_import_time_then_stable_id() {
    let root = directory("semver");
    fs::write(root.join("app.js"), "function hello() {}").unwrap();
    let old = candidate("../../languages", "javascript", "old", "0.9.0");
    let latest = candidate("../../languages", "javascript", "new", "0.10.0");
    let mut later = latest.clone();
    later.id = "later".into();
    later.created_at = "2026-11-08T02:00:00.000000Z".into();
    let mut tie = later.clone();
    tie.id = "aaa".into();
    let indexer = MultiProfileIndexer::new(vec![latest, later, old, tie], true).unwrap();
    let result = run(&indexer, &root);
    assert_eq!(result.selection["profiles"][0]["id"], "aaa");
    assert_eq!(result.selection["profiles"][0]["version"], "0.10.0");
    assert_eq!(result.index.snapshot.sources.len(), 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn manual_overlap_and_corrupt_candidates_are_rejected() {
    let old = candidate("../../languages", "javascript", "old", "0.9.0");
    let mut newer = old.clone();
    newer.id = "new".into();
    newer.package.version = "0.10.0".into();
    newer.checksum = newer.package.checksum().unwrap();
    assert!(MultiProfileIndexer::new(vec![old.clone(), newer], false).is_err());
    let mut corrupt = old;
    corrupt.checksum = "0".repeat(64);
    assert!(MultiProfileIndexer::new(vec![corrupt], true).is_err());
}
#[test]
fn no_matching_profile_preserves_an_inventory_report() {
    let root = directory("unknown");
    fs::write(root.join("source.dart"), "void main() {}").unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate("../../languages", "javascript", "js", "0.4.0")],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert!(result.index.snapshot.sources.is_empty());
    assert_eq!(result.index.snapshot.graph.nodes.len(), 1);
    assert_eq!(result.selection["profiles"], serde_json::json!([]));
    assert_eq!(
        result.selection["uncovered_files"],
        serde_json::json!(["source.dart"])
    );
    assert_eq!(result.selection["status"], "partial");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn independent_csharp_dependency_is_not_replaced_by_newer_root() {
    let root = directory("dependency-isolation");
    fs::write(
        root.join("A.cs"),
        "public class A { public int Value { get; set; } }",
    )
    .unwrap();
    fs::write(
        root.join("Index.razor"),
        "@code { private int counter = 0; }",
    )
    .unwrap();
    let mut csharp = candidate(
        "../../templates/dotnet-understanding",
        "csharp",
        "cs-new",
        "9.0.0",
    );
    csharp
        .package
        .models
        .iter_mut()
        .find(|m| m.manifest.id.0 == "csharp")
        .unwrap()
        .manifest
        .version = "9.0.0".into();
    csharp.checksum = csharp.package.checksum().unwrap();
    let razor = candidate(
        "../../templates/dotnet-understanding",
        "razor",
        "razor",
        "0.10.0",
    );
    let pinned = razor
        .package
        .models
        .iter()
        .find(|m| m.manifest.id.0 == "csharp")
        .unwrap()
        .manifest
        .version
        .clone();
    let result = run(
        &MultiProfileIndexer::new(vec![csharp, razor], true).unwrap(),
        &root,
    );
    let origins: Vec<_> = result
        .index
        .snapshot
        .graph
        .syntax_units
        .values()
        .filter(|u| u.language.0 == "csharp")
        .collect();
    assert!(
        origins
            .iter()
            .any(|u| u.path == "A.cs" && u.rule_version == "9.0.0")
    );
    assert!(
        origins
            .iter()
            .any(|u| u.path == "Index.razor" && u.rule_version == pinned)
    );
    assert!(
        origins
            .iter()
            .all(|u| u.path != "Index.razor" || u.rule_version != "9.0.0")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mixed_ts_tsx_cache_is_deterministic_and_invalidated_only_for_changed_package() {
    use nexora_indexer::{CacheFragment, FileCacheKey};
    use std::{cell::RefCell, collections::BTreeMap};
    let root = directory("cache");
    fs::write(root.join("a.ts"), "export const value: number = 1;").unwrap();
    fs::write(root.join("b.tsx"), "export const view = <div>Hello</div>;").unwrap();
    let ts = candidate(
        "../../templates/web-understanding",
        "typescript",
        "ts",
        "0.9.0",
    );
    let tsx = candidate("../../templates/web-understanding", "tsx", "tsx", "0.9.0");
    let records = RefCell::new(BTreeMap::<FileCacheKey, CacheFragment>::new());
    let mut read = |key: &FileCacheKey| Ok(records.borrow().get(key).cloned());
    let mut write = |key: &FileCacheKey, fragment: &CacheFragment| {
        records.borrow_mut().insert(key.clone(), fragment.clone());
        Ok(true)
    };
    let mut external = ExternalFileCache {
        read: &mut read,
        write: &mut write,
    };
    let index = |profiles: Vec<AnalysisProfileCandidate>, external: &mut ExternalFileCache<'_>| {
        MultiProfileIndexer::new(profiles, true)
            .unwrap()
            .index_repository_external(
                &root,
                TenantId("tenant".into()),
                RepositoryId("repo_mixed".into()),
                &mut MemoryStore::default(),
                external,
            )
            .unwrap()
    };
    let cold = index(vec![ts.clone(), tsx.clone()], &mut external);
    assert_eq!(cold.index.reuse.analyzed_files, 2);
    assert!(
        cold.index.snapshot.diagnostics.is_empty(),
        "{:?}",
        cold.index.snapshot.diagnostics
    );
    let warm = index(vec![tsx.clone(), ts.clone()], &mut external);
    assert_eq!(warm.index.reuse.reused_files, 2);
    assert_eq!(
        serde_json::to_value(&cold.index.snapshot).unwrap(),
        serde_json::to_value(&warm.index.snapshot).unwrap()
    );
    assert_eq!(cold.selection, warm.selection);
    let mut newer = ts.clone();
    newer.id = "ts-new".into();
    newer.package.version = "0.10.0".into();
    newer.checksum = newer.package.checksum().unwrap();
    let updated = index(vec![ts, tsx, newer], &mut external);
    assert_eq!(updated.index.reuse.reused_files, 1);
    assert_eq!(updated.index.reuse.analyzed_files, 1);
    assert_ne!(updated.index.snapshot.id, cold.index.snapshot.id);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn aggregate_sources_above_32_mib_complete_across_profiles() {
    let root = directory("budget");
    let content = " ".repeat(1_048_576);
    for index in 0..33 {
        fs::write(
            root.join(format!(
                "file-{index}.{}",
                if index % 2 == 0 { "js" } else { "css" }
            )),
            &content,
        )
        .unwrap();
    }
    let indexer = MultiProfileIndexer::new(
        vec![
            candidate("../../languages", "javascript", "js", "0.4.0"),
            candidate("../../templates/web-documents", "css", "css", "0.3.0"),
        ],
        true,
    )
    .unwrap();
    let mut read = |_: &nexora_indexer::FileCacheKey| Ok(None);
    let mut write = |_: &nexora_indexer::FileCacheKey, _: &nexora_indexer::CacheFragment| Ok(false);
    let result = indexer
        .index_repository_external(
            &root,
            TenantId("tenant".into()),
            RepositoryId("repo_mixed".into()),
            &mut MemoryStore::default(),
            &mut ExternalFileCache {
                read: &mut read,
                write: &mut write,
            },
        )
        .unwrap();
    assert_eq!(result.index.snapshot.sources.len(), 33);
    assert_eq!(result.index.reuse.read_files, 33);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_encoding_is_recorded_without_aborting_or_reusing_old_file_graph() {
    let root = directory("encoding");
    fs::write(root.join("Good.cs"), "public class Good {}").unwrap();
    fs::write(root.join("Legacy.cs"), b"public class Bad {} // caf\xe9").unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate(
            "../../templates/dotnet-understanding",
            "csharp",
            "cs",
            "0.9.0",
        )],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert!(result.index.snapshot.sources.contains_key("Good.cs"));
    assert!(!result.index.snapshot.sources.contains_key("Legacy.cs"));
    assert_eq!(result.selection["status"], "partial");
    assert_eq!(result.selection["skipped_files"][0]["path"], "Legacy.cs");
    assert_eq!(
        result.selection["skipped_files"][0]["analysis_status"],
        "unknown"
    );
    assert_eq!(
        result.selection["unknown_files"],
        serde_json::json!(["Legacy.cs"])
    );
    assert_eq!(result.selection["skipped_files"][0]["profile_id"], "cs");
    assert_eq!(
        result.selection["profiles"][0]["files"],
        serde_json::json!(["Good.cs"])
    );
    assert!(
        result
            .index
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "source_encoding" && d.path.as_deref() == Some("Legacy.cs"))
    );
    let original = result.index.snapshot.id;
    fs::write(root.join("Legacy.cs"), b"public class Bad {} // caf\xe8").unwrap();
    assert_ne!(
        original,
        run(&indexer, &root).index.snapshot.id,
        "raw skipped bytes must remain part of snapshot identity"
    );
    fs::write(root.join("Legacy.cs"), "public class Bad {}").unwrap();
    let repaired = run(&indexer, &root);
    assert_eq!(repaired.selection["status"], "complete");
    assert!(repaired.index.snapshot.sources.contains_key("Legacy.cs"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn all_matching_files_with_invalid_encoding_are_explicitly_partial() {
    let root = directory("all-encoding");
    fs::write(root.join("Legacy.cs"), [0xe9]).unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate(
            "../../templates/dotnet-understanding",
            "csharp",
            "cs",
            "0.9.0",
        )],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert!(result.index.snapshot.sources.is_empty());
    assert_eq!(result.selection["status"], "partial");
    assert_eq!(
        result.selection["skipped_files"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        result.selection["profiles"][0]["files"],
        serde_json::json!([])
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn skipped_sources_above_32_mib_are_reported_without_aborting() {
    let root = directory("encoding-budget");
    let bytes = vec![0xe9; 1_048_576];
    for i in 0..33 {
        fs::write(root.join(format!("bad{i}.js")), &bytes).unwrap();
    }
    let indexer = MultiProfileIndexer::new(
        vec![candidate("../../languages", "javascript", "js", "0.4.0")],
        true,
    )
    .unwrap();
    let mut read = |_: &nexora_indexer::FileCacheKey| Ok(None);
    let mut write = |_: &nexora_indexer::FileCacheKey, _: &nexora_indexer::CacheFragment| Ok(false);
    let mut store = MemoryStore::default();
    let result = indexer.index_repository_external(
        &root,
        TenantId("tenant".into()),
        RepositoryId("repo_encoding".into()),
        &mut store,
        &mut ExternalFileCache {
            read: &mut read,
            write: &mut write,
        },
    );
    let result = result.unwrap();
    assert_eq!(
        result.selection["skipped_files"].as_array().unwrap().len(),
        33
    );
    assert_eq!(result.index.snapshot.diagnostics.len(), 33);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_lexical_regions_are_file_diagnostics_not_repository_failures() {
    let root = directory("lexical-rejection");
    fs::write(root.join("Good.html"), "<p>Hello</p>").unwrap();
    fs::write(root.join("Svg.html"), "<svg><path d=\"M0 0\"/></svg>").unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate(
            "../../templates/web-documents",
            "html",
            "html",
            "0.3.0",
        )],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert!(result.index.snapshot.sources.contains_key("Good.html"));
    assert!(!result.index.snapshot.sources.contains_key("Svg.html"));
    assert_eq!(result.selection["status"], "partial");
    assert_eq!(result.selection["skipped_files"][0]["path"], "Svg.html");
    assert!(
        result.selection["skipped_files"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("unsupported_svg")
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_cached_valid_source_cannot_survive_as_a_phantom_graph_after_encoding_failure() {
    use nexora_indexer::{CacheFragment, FileCacheKey};
    use std::{cell::RefCell, collections::BTreeMap};
    let root = directory("encoding-cache");
    fs::write(root.join("a.js"), "function good() {}").unwrap();
    fs::write(root.join("b.js"), "function previous() {}").unwrap();
    let records = RefCell::new(BTreeMap::<FileCacheKey, CacheFragment>::new());
    let mut read = |key: &FileCacheKey| Ok(records.borrow().get(key).cloned());
    let mut write = |key: &FileCacheKey, fragment: &CacheFragment| {
        records.borrow_mut().insert(key.clone(), fragment.clone());
        Ok(true)
    };
    let mut external = ExternalFileCache {
        read: &mut read,
        write: &mut write,
    };
    let indexer = MultiProfileIndexer::new(
        vec![candidate("../../languages", "javascript", "js", "0.4.0")],
        true,
    )
    .unwrap();
    let mut index = || {
        indexer
            .index_repository_external(
                &root,
                TenantId("tenant".into()),
                RepositoryId("repo_encoding_cache".into()),
                &mut MemoryStore::default(),
                &mut external,
            )
            .unwrap()
    };
    let cold = index();
    assert_eq!(cold.index.reuse.retained_files, 2);
    fs::write(root.join("b.js"), b"function previous() {} // \xe9").unwrap();
    let partial = index();
    assert_eq!(partial.index.reuse.reused_files, 1);
    assert!(!partial.index.snapshot.sources.contains_key("b.js"));
    assert!(
        !partial
            .index
            .snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.name == "previous")
    );
    fs::write(root.join("b.js"), "function previous() {}").unwrap();
    let restored = index();
    assert_eq!(restored.index.reuse.reused_files, 2);
    assert_eq!(restored.index.snapshot.id, cold.index.snapshot.id);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn converted_sources_retain_original_encoding_and_raw_checksum_in_the_report() {
    use sha2::{Digest, Sha256};
    let root = directory("encoding-provenance");
    let mut bytes = vec![0xff, 0xfe];
    for unit in "public class Good {}".encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    fs::write(root.join("Good.cs"), &bytes).unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate(
            "../../templates/dotnet-understanding",
            "csharp",
            "cs",
            "0.9.0",
        )],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert_eq!(result.selection["decoded_files"][0]["path"], "Good.cs");
    assert_eq!(result.selection["decoded_files"][0]["encoding"], "UTF-16LE");
    assert_eq!(
        result.selection["decoded_files"][0]["checksum"],
        format!("{:x}", Sha256::digest(bytes))
    );
    assert_eq!(
        result.index.snapshot.sources["Good.cs"],
        "public class Good {}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_analysis_budget_failure_is_reported_and_later_files_are_published() {
    let root = directory("file-budget-recovery");
    fs::write(root.join("a-vendor.js"), "x;".repeat(100_001)).unwrap();
    fs::write(root.join("z-app.js"), "function retained() {}").unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate("../../languages", "javascript", "js", "0.4.0")],
        true,
    )
    .unwrap();
    let result = run(&indexer, &root);
    assert!(result.index.snapshot.sources.contains_key("z-app.js"));
    assert!(!result.index.snapshot.sources.contains_key("a-vendor.js"));
    assert_eq!(result.selection["skipped_files"][0]["path"], "a-vendor.js");
    assert!(
        result.selection["skipped_files"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("budget")
    );
    assert!(
        result
            .index
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.path.as_deref() == Some("a-vendor.js"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn storage_failure_is_not_misreported_as_an_unsupported_file() {
    let root = directory("storage-failure");
    fs::write(root.join("app.js"), "function retained() {}").unwrap();
    let indexer = MultiProfileIndexer::new(
        vec![candidate("../../languages", "javascript", "js", "0.4.0")],
        true,
    )
    .unwrap();
    let mut read = |_: &nexora_indexer::FileCacheKey| {
        Err(nexora_core::NexoraError::Storage(
            "temporarily unavailable".into(),
        ))
    };
    let mut write = |_: &nexora_indexer::FileCacheKey, _: &nexora_indexer::CacheFragment| Ok(false);
    let result = indexer.index_repository_external(
        &root,
        TenantId("tenant".into()),
        RepositoryId("repo_storage".into()),
        &mut MemoryStore::default(),
        &mut ExternalFileCache {
            read: &mut read,
            write: &mut write,
        },
    );
    assert!(matches!(result, Err(nexora_core::NexoraError::Storage(_))));
    fs::remove_dir_all(root).unwrap();
}

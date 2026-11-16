use nexora_graph::{EdgeKind, ResolutionState};
use nexora_indexer::{AnalysisResult, Indexer};
use nexora_language::LanguageDefinition;
use serde_json::json;
use std::{fs, path::Path};

fn language(id: &str, maximum: u16) -> LanguageDefinition {
    let mut l =
        LanguageDefinition::load(&Path::new("../../templates/semantic-molds").join(id)).unwrap();
    let (open, close) = if id == "brace_mold" {
        ("[", "]")
    } else {
        ("(", ")")
    };
    let mut grammar = serde_json::to_value(&l.grammar).unwrap();
    grammar["schema_version"] = json!(8);
    grammar["groups"]
        .as_array_mut()
        .unwrap()
        .push(json!({"open":open,"close":close,"kind":"delimiter"}));
    let expression = &mut grammar["contexts"]["expression"]["expression"];
    expression["application"].as_array_mut().unwrap().push(json!({"open":open,"close":close,"kind":"delimiter","separator":{"text":",","kind":"delimiter"},"precedence":80,"expression_kind":"index","max_arguments":maximum}));
    expression["assignment_targets"]
        .as_array_mut()
        .unwrap()
        .push(json!("index"));
    l.grammar = serde_json::from_value(grammar).unwrap();
    let mut graph = serde_json::to_value(&l.graph).unwrap();
    graph["schema_version"] = json!(5);
    graph["expression_bindings"] = json!({"expression":{"target_kinds":["Function","Variable","Parameter"],"callable_kinds":["Function"],"unwrap_groups":true,"mutating_unary_operators":["++"]}});
    l.graph = serde_json::from_value(graph).unwrap();
    l.validate().unwrap();
    l
}
fn analyze(l: LanguageDefinition, source: &str, name: &str) -> AnalysisResult {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-index-shape-{}-{name}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("public.{}", l.manifest.extensions[0]));
    fs::write(&path, source).unwrap();
    let result = Indexer::new(l).analyze(&path).unwrap();
    fs::remove_dir_all(directory).unwrap();
    result
}
#[test]
fn two_delimiter_syntaxes_build_index_shapes_without_inventing_calls() {
    for (id, source, text) in [
        (
            "brace_mold",
            "VAR data; VAR result=data[1 + 2 * 3];",
            "data[1 + 2 * 3]",
        ),
        (
            "keyword_mold",
            "LET data; LET result BECOMES data(1 ADD 2 TIMES 3);",
            "data(1 ADD 2 TIMES 3)",
        ),
    ] {
        let r = analyze(language(id, 1), source, id);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let index = r
            .ast
            .nodes
            .iter()
            .find(|n| &source[n.span.start.offset..n.span.end.offset] == text)
            .unwrap();
        let shape = index.expression.as_ref().unwrap();
        assert_eq!(serde_json::to_value(shape.kind).unwrap(), json!("index"));
        assert_eq!(shape.operands.len(), 2);
        let operand = r
            .ast
            .nodes
            .iter()
            .find(|n| n.id == shape.operands[1])
            .unwrap();
        assert!(matches!(
            operand.expression.as_ref().unwrap().operator.as_deref(),
            Some("+") | Some("ADD")
        ));
        assert!(r.graph.edges.iter().all(|e| e.kind != EdgeKind::Calls));
        let graph: nexora_graph::CodeGraph =
            serde_json::from_value(serde_json::to_value(&r.graph).unwrap()).unwrap();
        nexora_graph::GraphIntegrityValidator::validate(&graph).unwrap();
    }
}
#[test]
fn index_arity_empty_missing_closer_and_schema_are_checked() {
    let l = language("brace_mold", 1);
    for (label, source) in [
        ("empty", "VAR data; VAR result=data[]; VAR after=1;"),
        ("many", "VAR data; VAR result=data[0,1]; VAR after=1;"),
        ("missing", "VAR data; VAR result=data[0; VAR after=1;"),
    ] {
        let r = analyze(l.clone(), source, label);
        assert!(!r.diagnostics.is_empty());
    }
    let r = analyze(
        language("brace_mold", 2),
        "VAR data; VAR result=data[0,1];",
        "two",
    );
    assert!(r.diagnostics.is_empty());
    assert!(
        r.ast.nodes.iter().any(
            |n| n
                .expression
                .as_ref()
                .is_some_and(|s| serde_json::to_value(s.kind).unwrap() == json!("index")
                    && s.operands.len() == 3)
        )
    );
    let mut old = l;
    old.grammar.schema_version = 7;
    assert!(old.validate().is_err());
}
#[test]
fn element_writes_do_not_reassign_receiver_bindings_or_prove_indexed_calls() {
    let source = "FUNC target() {} VAR data; EVAL data[0]=target; EVAL target(); EVAL data[0]();";
    let r = analyze(language("brace_mold", 1), source, "writes");
    assert_eq!(r.diagnostics.len(), 1);
    assert_eq!(r.diagnostics[0].stage, "expression-resolution");
    assert_eq!(r.diagnostics[0].message, "indirect_or_member_callee");
    let calls: Vec<_> = r
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls
            .iter()
            .filter(|e| e.resolution == ResolutionState::Resolved)
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|e| e.resolution == ResolutionState::Unresolved)
            .count(),
        1
    );
}

#[test]
fn nested_indices_keep_absolute_unicode_ranges_and_calls_remain_distinct() {
    let source = "FUNC target() {}\r\nVAR datos; VAR índice=0; VAR result=datos[índice][target()];";
    let r = analyze(language("brace_mold", 1), source, "nested");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    let indexed: Vec<_> = r
        .ast
        .nodes
        .iter()
        .filter(|n| {
            n.expression
                .as_ref()
                .is_some_and(|s| serde_json::to_value(s.kind).unwrap() == json!("index"))
        })
        .collect();
    assert_eq!(indexed.len(), 2);
    assert!(
        indexed
            .iter()
            .any(|n| &source[n.span.start.offset..n.span.end.offset] == "datos[índice]")
    );
    let outer = indexed
        .iter()
        .find(|n| &source[n.span.start.offset..n.span.end.offset] == "datos[índice][target()]")
        .unwrap();
    let base = &outer.expression.as_ref().unwrap().operands[0];
    assert!(indexed.iter().any(|n| &n.id == base));
    assert_eq!(outer.span.start.line, 2);
    let calls: Vec<_> = r
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].resolution, ResolutionState::Resolved);
}

#[test]
fn operation_kind_and_limits_are_validated_before_parsing() {
    let original = language("brace_mold", 1);
    for (field, value) in [
        ("expression_kind", json!("literal")),
        ("max_arguments", json!(0)),
        ("max_arguments", json!(1025)),
    ] {
        let mut l = original.clone();
        let mut grammar = serde_json::to_value(&l.grammar).unwrap();
        let operation = grammar["contexts"]["expression"]["expression"]["application"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        operation[field] = value;
        l.grammar = serde_json::from_value(grammar).unwrap();
        assert!(l.validate().is_err());
    }
    let mut missing = original;
    missing
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application
        .pop();
    assert!(missing.validate().is_err());
}

#[test]
fn index_arity_policy_changes_invalidate_cache_and_match_full_analysis() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, MemoryStore};
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-index-cache-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("public.nxb");
    fs::write(&path, "VAR data; VAR value=data[0];").unwrap();
    let l = language("brace_mold", 1);
    let indexer = Indexer::new(l.clone());
    let tenant = TenantId("public-index".into());
    let mut store = MemoryStore::default();
    let mut cache = IndexCache::default();
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    fs::write(&path, "VAR data; VAR value=data[0,1];").unwrap();
    let partial = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert!(!partial.snapshot.diagnostics.is_empty());
    let mut changed = l;
    changed
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application
        .last_mut()
        .unwrap()
        .max_arguments = Some(2);
    let changed = Indexer::new(changed);
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert!(refreshed.snapshot.diagnostics.is_empty());
    assert_ne!(refreshed.snapshot.id, partial.snapshot.id);
    let full = changed
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(&refreshed.snapshot).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}

use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use std::path::Path;
fn language() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../languages/typescript")).unwrap()
}
fn definition() -> nexora_language::LexerDefinition {
    let mut d = serde_json::to_value(language().lexer).unwrap();
    d["schema_version"] = serde_json::json!(3);
    d["keyword_after_exclusions"] = serde_json::json!([]);
    for region in d["regions"].as_object_mut().unwrap().values_mut() {
        region["token_rules"] = serde_json::json!([]);
        region["token_kind"] = serde_json::Value::Null;
        region["trivia"] = false.into();
    }
    let tokens = d["tokens"].as_array_mut().unwrap();
    for token in tokens.iter_mut() {
        token["follow_exclude"] = serde_json::Value::Null;
    }
    for token in tokens.iter_mut() {
        token["reject_on_context_mismatch"] = serde_json::json!(false);
    }
    tokens.retain(|t| t["kind"] != "unsupported_interpolation");
    let t = tokens.iter_mut().find(|t| t["kind"] == "template").unwrap();
    t["pattern"] = serde_json::json!("`");
    t["region"] = serde_json::json!("template");
    d["regions"] = serde_json::json!({
     "template":{"close":"`","escape":"\\","children":[{"pattern":"\\$\\{","region":"code"}]},
     "code":{"close":"}","children":[{"pattern":"\\{","region":"brace"},{"pattern":"`","region":"template"},{"pattern":"\"","region":"quote"},{"pattern":"/\\*","region":"comment"}]},
     "brace":{"close":"}","children":[{"pattern":"\\{","region":"brace"},{"pattern":"`","region":"template"},{"pattern":"\"","region":"quote"}]},
     "quote":{"close":"\"","escape":"\\"},"comment":{"close":"*/"}
    });
    serde_json::from_value(d).unwrap()
}
#[test]
fn nested_regions_preserve_boundaries_parentage_and_unicode_positions() {
    let source =
        "const t = `café\r\n${fn({value: `inner ${x}`}) /* } ` */ + \"}\"} fin`; const after = 2;";
    let tokens = Lexer::new(&definition()).unwrap().tokenize(source).unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
        source
    );
    let t = tokens.iter().find(|t| t.kind == "template").unwrap();
    let json = serde_json::to_value(t).unwrap();
    let regions = json["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 7);
    for r in regions {
        assert_eq!(r["closed"], true);
        let start = r["span"]["start"]["offset"].as_u64().unwrap() as usize;
        let mut p = nexora_core::SourcePosition::default();
        p.advance(&source[..start]);
        assert_eq!(serde_json::to_value(p).unwrap(), r["span"]["start"]);
    }
    let root = &regions[0];
    assert!(root["parent"].is_null());
    assert_eq!(regions[1]["parent"], root["id"]);
    assert!(
        !tokens
            .iter()
            .any(|t| t.text == "value" || t.text == "inner")
    );
    assert!(tokens.iter().any(|t| t.text == "after"));
}
#[test]
fn alternate_delimiters_use_the_same_stack_and_unterminated_regions_are_explicit() {
    let mut d = serde_json::to_value(definition()).unwrap();
    let t = d["tokens"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["kind"] == "template")
        .unwrap();
    t["pattern"] = serde_json::json!("~\\[");
    d["regions"]["template"]["close"] = serde_json::json!("]~");
    d["regions"]["template"]["children"][0]["pattern"] = serde_json::json!("@\\(");
    d["regions"]["code"]["close"] = serde_json::json!(")");
    let d = serde_json::from_value(d).unwrap();
    let tokens = Lexer::new(&d)
        .unwrap()
        .tokenize("~[text @(x) end]~")
        .unwrap();
    let j = serde_json::to_value(&tokens[0]).unwrap();
    assert_eq!(j["regions"].as_array().unwrap().len(), 2);
    assert_eq!(j["regions"][0]["closed"], true);
    let tokens = Lexer::new(&d).unwrap().tokenize("~[text @(x").unwrap();
    let j = serde_json::to_value(&tokens[0]).unwrap();
    assert_eq!(j["regions"][0]["closed"], false);
    assert_eq!(j["regions"][1]["closed"], false);
}
fn analyze(source: &str, name: &str) -> nexora_indexer::AnalysisResult {
    let mut l = language();
    l.lexer = definition();
    l.graph.schema_version = 7;
    l.graph
        .mappings
        .insert("lexical_region".into(), "LexicalRegion".into());
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-composed-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.ts");
    std::fs::write(&path, source).unwrap();
    let r = nexora_indexer::Indexer::new(l).analyze(&path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    r
}
#[test]
fn composed_regions_have_real_cst_ast_graph_owners_and_do_not_declare_literal_names() {
    let source = "const t = `hello ${`const leaked = 1; ${x}`} end`; const after = 2;";
    let r = analyze(source, "graph");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(!r.symbols.symbols.iter().any(|n| n.name == "leaked"));
    let regions: Vec<_> = r
        .graph
        .nodes
        .iter()
        .filter(|n| format!("{:?}", n.kind) == "LexicalRegion")
        .collect();
    assert_eq!(regions.len(), 4);
    assert!(!r.symbols.complete);
    for node in regions {
        let edge = r
            .graph
            .edges
            .iter()
            .find(|e| e.kind == nexora_graph::EdgeKind::Contains && e.target == node.id)
            .unwrap();
        assert_eq!(edge.evidence.span, node.evidence.as_ref().unwrap().span);
        assert!(r.ast.nodes.iter().any(|n| n.id == edge.evidence.ast_id));
    }
}
#[test]
fn eof_permission_cannot_close_a_parent_over_an_unterminated_child() {
    let mut d = serde_json::to_value(definition()).unwrap();
    d["regions"]["template"]["eof_closes"] = serde_json::json!(true);
    let d = serde_json::from_value(d).unwrap();
    let tokens = Lexer::new(&d).unwrap().tokenize("`hello ${x").unwrap();
    let j = serde_json::to_value(&tokens[0]).unwrap();
    assert_eq!(j["regions"][0]["closed"], false);
}
#[test]
fn unclosed_regions_remain_partial_in_the_graph_and_never_resolve_calls() {
    let r = analyze("const t = `hello ${x", "partial");
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("unterminated lexical region"))
    );
    assert!(!r.ast.complete);
    assert!(
        r.graph
            .syntax_coverage
            .values()
            .any(|c| c.partial_regions > 0)
    );
    assert!(
        !r.graph
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::Calls
                && e.resolution == nexora_graph::ResolutionState::Resolved)
    );
}
#[test]
fn region_configuration_and_resource_limits_are_validated_and_shared() {
    let original = serde_json::to_value(definition()).unwrap();
    for mode in [
        "version",
        "missing",
        "empty-close",
        "empty-transition",
        "bad-depth",
        "bad-steps",
    ] {
        let mut value = original.clone();
        match mode {
            "version" => value["schema_version"] = serde_json::json!(2),
            "missing" => {
                value["regions"]["template"]["children"][0]["region"] = serde_json::json!("missing")
            }
            "empty-close" => value["regions"]["template"]["close"] = serde_json::json!(""),
            "empty-transition" => {
                value["regions"]["template"]["children"][0]["pattern"] = serde_json::json!("a*")
            }
            "bad-depth" => value["max_region_depth"] = serde_json::json!(0),
            _ => value["max_region_steps"] = serde_json::json!(2_000_001),
        }
        let d = serde_json::from_value(value).unwrap();
        assert!(Lexer::new(&d).is_err(), "{mode}");
    }
    let mut d = definition();
    d.max_region_depth = 8;
    assert!(
        Lexer::new(&d)
            .unwrap()
            .tokenize(&format!("`${{{}x{}}}`", "{".repeat(30), "}".repeat(30)))
            .is_err()
    );
    let mut d = definition();
    d.max_region_steps = 8;
    assert!(
        Lexer::new(&d)
            .unwrap()
            .tokenize("`abc` `def` `ghi` `jkl`")
            .is_err()
    );
}
#[test]
fn node_budget_applies_across_multiple_top_level_region_tokens() {
    let mut value = serde_json::to_value(definition()).unwrap();
    value["max_region_nodes"] = serde_json::json!(3);
    let d = serde_json::from_value(value).unwrap();
    assert!(Lexer::new(&d).unwrap().tokenize("`a` `b` `c` `d`").is_err());
}
#[test]
fn declared_unsupported_transitions_fail_closed_instead_of_claiming_wrong_boundaries() {
    let mut d = serde_json::to_value(definition()).unwrap();
    d["regions"]["code"]["children"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"pattern":"/","region":"code","reject":true}));
    let d = serde_json::from_value(d).unwrap();
    assert!(
        Lexer::new(&d)
            .unwrap()
            .tokenize("`hello ${ /}/.test(x) } end`")
            .is_err()
    );
}
#[test]
fn contextual_zero_width_transitions_are_invalid_even_when_empty_input_does_not_match() {
    for pattern in [r"\b", r"a|\b", r"^", r"(?:x)?"] {
        let mut d = serde_json::to_value(definition()).unwrap();
        d["regions"]["template"]["children"][0]["pattern"] = serde_json::json!(pattern);
        let d = serde_json::from_value(d).unwrap();
        assert!(Lexer::new(&d).is_err(), "{pattern}");
    }
}
#[test]
fn interior_line_comments_accept_each_declared_line_terminator() {
    for (i, end) in ["\n", "\r\n", "\r", "\u{2028}", "\u{2029}"]
        .iter()
        .enumerate()
    {
        let l = language();
        let source = format!("const t = `hi ${{ // comment{end} x }} end`; const after = 2;");
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("nexora-linecomment-{}-{i}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let p = root.join("source.ts");
        std::fs::write(&p, &source).unwrap();
        let r = nexora_indexer::Indexer::new(l).analyze(&p).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(r.diagnostics.is_empty(), "{end:?}: {:?}", r.diagnostics);
        assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
    }
}
#[test]
fn region_ids_and_host_owners_survive_cst_ast_graph_and_incremental_cache() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let mut l = language();
    l.lexer = definition();
    l.graph.schema_version = 7;
    l.graph
        .mappings
        .insert("lexical_region".into(), "LexicalRegion".into());
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-region-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("a.ts"),
        "const first = `a`; const second = `b ${x}`;",
    )
    .unwrap();
    std::fs::write(root.join("b.ts"), "const first = `a`;").unwrap();
    let indexer = Indexer::new(l);
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 2);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    let graph = &warm.snapshot.graph;
    let ids: std::collections::BTreeSet<_> = graph.nodes.iter().map(|n| &n.id).collect();
    for node in &graph.nodes {
        if let Some(region) = &node.lexical_region {
            assert_eq!(region.id, node.id);
            if let Some(parent) = &region.parent {
                assert!(ids.contains(parent));
                assert!(
                    graph
                        .edges
                        .iter()
                        .any(|e| e.kind == nexora_graph::EdgeKind::Contains
                            && &e.source == parent
                            && e.target == node.id)
                );
            } else {
                let owner = graph
                    .edges
                    .iter()
                    .find(|e| e.kind == nexora_graph::EdgeKind::Contains && e.target == node.id)
                    .unwrap();
                let owner = graph.nodes.iter().find(|n| n.id == owner.source).unwrap();
                assert!(matches!(owner.name.as_str(), "first" | "second"));
            }
        }
    }
    std::fs::write(
        root.join("a.ts"),
        "const first = `a ${`nested`}`; const second = `b ${x}`;",
    )
    .unwrap();
    let changed = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.reused_files, 1);
    let full = indexer
        .index(&root, TenantId("test".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&changed.snapshot).unwrap(),
        serde_json::to_value(&full).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}

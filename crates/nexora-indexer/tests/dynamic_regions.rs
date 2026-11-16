use nexora_language::{LanguageDefinition, LexerDefinition};
use nexora_lexer::Lexer;
use std::path::Path;
fn definition() -> LexerDefinition {
    let l = LanguageDefinition::load(Path::new("../../languages/tsx")).unwrap();
    let mut d = serde_json::to_value(l.lexer).unwrap();
    d["schema_version"] = serde_json::json!(4);
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
    tokens.retain(|t| t["kind"] != "unsupported_markup_boundary");
    let t = tokens.iter_mut().find(|t| t["kind"] == "jsx_atom").unwrap();
    t["pattern"] = serde_json::json!(r"<(?P<tag>[A-Za-z][A-Za-z0-9_.:-]*)");
    t["region"] = serde_json::json!("element");
    d["regions"]["element"] = serde_json::json!({"close":"</","close_capture":"tag","close_tail":"\\s*>","recovery_prefixes":["</"],"header":{"end":">","terminal":"/>","children":[{"pattern":"\"","region":"attribute_quote"},{"pattern":"\\{","region":"code"}]},"children":[{"pattern":"<(?P<tag>[A-Za-z][A-Za-z0-9_.:-]*)","region":"element"},{"pattern":"\\{","region":"code"},{"pattern":"</","region":"element","reject":true}]});
    d["regions"]["attribute_quote"] = serde_json::json!({"close":"\""});
    serde_json::from_value(d).unwrap()
}
#[test]
fn captured_delimiters_track_deep_elements_headers_attributes_and_following_code() {
    let source = format!(
        "const Button = () => <Root title=\"a > b\" onClick={{() => act()}}>{}hello; const leaked = 1;{}<Input title=\"x\" /></Root>; const after = 2;",
        "<Box>".repeat(30),
        "</Box>".repeat(30)
    );
    let tokens = Lexer::new(&definition())
        .unwrap()
        .tokenize(&source)
        .unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
        source
    );
    assert!(tokens.iter().any(|t| t.text == "after"));
    assert!(!tokens.iter().any(|t| t.text == "leaked"));
    let t = tokens.iter().find(|t| t.kind == "jsx_atom").unwrap();
    let j = serde_json::to_value(t).unwrap();
    let regions = j["regions"].as_array().unwrap();
    assert_eq!(
        regions.iter().filter(|r| r["mode"] == "element").count(),
        32
    );
    assert!(regions.iter().all(|r| r["closed"] == true));
    assert!(regions.iter().any(|r| r["self_closed"] == true));
    let root = &regions[0];
    assert!(root["header"].is_object());
    let capture = &root["captures"]["tag"];
    let a = capture["start"]["offset"].as_u64().unwrap() as usize;
    let b = capture["end"]["offset"].as_u64().unwrap() as usize;
    assert_eq!(&source[a..b], "Root");
}
#[test]
fn ancestor_closing_recovers_unclosed_child_without_consuming_the_next_statement() {
    let tokens = Lexer::new(&definition())
        .unwrap()
        .tokenize("const View = <Outer><Inner>text</Outer>; const after = 2;")
        .unwrap();
    let token = tokens.iter().find(|t| t.kind == "jsx_atom").unwrap();
    let j = serde_json::to_value(token).unwrap();
    assert_eq!(j["regions"][0]["closed"], true);
    assert_eq!(j["regions"][1]["closed"], false);
    assert!(tokens.iter().any(|t| t.text == "after"));
}
#[test]
fn captured_names_are_literal_and_similar_names_do_not_close_the_frame() {
    let good = Lexer::new(&definition())
        .unwrap()
        .tokenize("const V = <A.B>text</A.B >;")
        .unwrap();
    let j = serde_json::to_value(good.iter().find(|t| t.kind == "jsx_atom").unwrap()).unwrap();
    assert_eq!(j["regions"][0]["closed"], true);
    assert!(
        Lexer::new(&definition())
            .unwrap()
            .tokenize("const V = <A.B>text</AxB>;")
            .is_err()
    );
    assert!(
        Lexer::new(&definition())
            .unwrap()
            .tokenize("const V = <Box>text</BoxExtra>;")
            .is_err()
    );
}
#[test]
fn a_second_captured_delimiter_syntax_uses_the_same_engine() {
    let mut d = serde_json::to_value(definition()).unwrap();
    let pattern = r"#\[(?P<tag>[A-Za-z]+)\]";
    let token = d["tokens"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["kind"] == "jsx_atom")
        .unwrap();
    token["pattern"] = serde_json::json!(pattern);
    d["regions"]["element"]["header"] = serde_json::Value::Null;
    d["regions"]["element"]["close"] = serde_json::json!("[/");
    d["regions"]["element"]["close_tail"] = serde_json::json!(r"\]");
    d["regions"]["element"]["recovery_prefixes"] = serde_json::json!(["[/"]);
    d["regions"]["element"]["children"] =
        serde_json::json!([{"pattern":pattern,"region":"element"}]);
    let d = serde_json::from_value(d).unwrap();
    let source = "const t = #[Outer]a #[Inner]b[/Inner] c[/Outer]; const after = 2;";
    let tokens = Lexer::new(&d).unwrap().tokenize(source).unwrap();
    let region =
        serde_json::to_value(tokens.iter().find(|t| t.kind == "jsx_atom").unwrap()).unwrap();
    assert_eq!(region["regions"].as_array().unwrap().len(), 2);
    assert!(
        region["regions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["closed"] == true)
    );
    assert!(tokens.iter().any(|t| t.text == "after"));
}
#[test]
fn dynamic_configuration_captures_headers_and_version_are_checked() {
    let original = serde_json::to_value(definition()).unwrap();
    for mode in [
        "schema",
        "unknown-capture",
        "missing-child-capture",
        "empty-header",
        "empty-terminal",
        "bad-tail",
        "too-many-captures",
    ] {
        let mut d = original.clone();
        match mode {
            "schema" => d["schema_version"] = serde_json::json!(3),
            "unknown-capture" => {
                d["regions"]["element"]["close_capture"] = serde_json::json!("missing")
            }
            "missing-child-capture" => {
                d["regions"]["element"]["children"][0]["pattern"] = serde_json::json!("<Box")
            }
            "empty-header" => d["regions"]["element"]["header"]["end"] = serde_json::json!(""),
            "empty-terminal" => {
                d["regions"]["element"]["header"]["terminal"] = serde_json::json!("")
            }
            "bad-tail" => d["regions"]["element"]["close_tail"] = serde_json::json!("("),
            _ => {
                let text = (0..33).map(|i| format!("(?P<c{i}>x)")).collect::<String>();
                d["regions"]["element"]["children"][0]["pattern"] =
                    serde_json::json!(format!("{text}(?P<tag>Box)"));
            }
        }
        let d = serde_json::from_value(d).unwrap();
        assert!(Lexer::new(&d).is_err(), "{mode}");
    }
    let mut d = original.clone();
    d["tokens"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["kind"] == "jsx_atom")
        .unwrap()["pattern"] = serde_json::json!("<(?P<tag>Box)?");
    let d = serde_json::from_value(d).unwrap();
    assert!(
        Lexer::new(&d)
            .unwrap()
            .tokenize("const x = < title")
            .is_err()
    );
}
#[test]
fn eof_in_a_header_is_partial_and_no_header_eof_permission_can_close_it() {
    let mut d = serde_json::to_value(definition()).unwrap();
    d["regions"]["element"]["eof_closes"] = serde_json::json!(true);
    let d = serde_json::from_value(d).unwrap();
    let tokens = Lexer::new(&d)
        .unwrap()
        .tokenize("const x = <Box title=\"unterminated")
        .unwrap();
    let j = serde_json::to_value(tokens.iter().find(|t| t.kind == "jsx_atom").unwrap()).unwrap();
    assert_eq!(j["regions"][0]["closed"], false);
    assert_eq!(
        j["regions"][0]["content_span"]["start"],
        j["regions"][0]["content_span"]["end"]
    );
}
#[test]
fn ascii_case_matching_is_opt_in_and_does_not_change_capture_evidence() {
    let mut d = serde_json::to_value(definition()).unwrap();
    d["regions"]["element"]["close_ascii_case_insensitive"] = serde_json::json!(true);
    let d = serde_json::from_value(d).unwrap();
    let tokens = Lexer::new(&d)
        .unwrap()
        .tokenize("const x = <BoX>text</box>;")
        .unwrap();
    let j = serde_json::to_value(tokens.iter().find(|t| t.kind == "jsx_atom").unwrap()).unwrap();
    assert_eq!(j["regions"][0]["closed"], true);
    assert!(
        Lexer::new(&definition())
            .unwrap()
            .tokenize("const x = <BoX>text</box>;")
            .is_err()
    );
}
#[test]
fn opening_right_context_disambiguates_generic_prefixes_without_consuming_the_suffix() {
    let mut d = serde_json::to_value(definition()).unwrap();
    d["regions"]["element"]["opening_follow"] = serde_json::json!(r"(?:[\s/>]|$)");
    let d = serde_json::from_value(d).unwrap();
    let tokens = Lexer::new(&d)
        .unwrap()
        .tokenize("const f = <T,>(x) => x; const v = <Box/>;")
        .unwrap();
    assert_eq!(tokens.iter().filter(|t| t.kind == "jsx_atom").count(), 1);
    let r = serde_json::to_value(tokens.iter().find(|t| t.kind == "jsx_atom").unwrap()).unwrap();
    assert_eq!(r["text"], "<Box/>");
    assert_eq!(r["regions"][0]["self_closed"], true);
}
#[test]
fn dynamic_header_metadata_and_partial_recovery_are_preserved_in_real_graph_nodes() {
    use nexora_graph::{EdgeKind, NodeKind};
    use nexora_indexer::Indexer;
    let l = LanguageDefinition::load(Path::new("../../languages/tsx")).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-dynamic-graph-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let p = root.join("source.tsx");
    let source =
        "const View = <Outer title=\"a > b\" enabled={flag}><Inner>text</Outer>; const after = 2;";
    std::fs::write(&p, source).unwrap();
    let r = Indexer::new(l).analyze(&p).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(!r.ast.complete);
    assert!(!r.symbols.complete);
    let nodes: std::collections::BTreeMap<_, _> =
        r.graph.nodes.iter().map(|n| (&n.id, n)).collect();
    let outer = r
        .graph
        .nodes
        .iter()
        .find(|n| {
            n.lexical_region.as_ref().is_some_and(|m| {
                m.captures
                    .get("tag")
                    .is_some_and(|s| &source[s.start.offset..s.end.offset] == "Outer")
            })
        })
        .unwrap();
    let m = outer.lexical_region.as_ref().unwrap();
    assert!(m.closed);
    let h = m.header.unwrap();
    assert_eq!(
        &source[h.start.offset..h.end.offset],
        "<Outer title=\"a > b\" enabled={flag}>"
    );
    for n in r
        .graph
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::LexicalRegion)
    {
        let m = n.lexical_region.as_ref().unwrap();
        assert_eq!(m.id, n.id);
        if let Some(parent) = &m.parent {
            assert!(nodes.contains_key(parent));
        }
        assert!(r.graph.edges.iter().any(|e| e.kind == EdgeKind::Contains
            && e.target == n.id
            && e.evidence.span == m.span));
    }
    assert!(r.graph.nodes.iter().any(|n| {
        n.lexical_region
            .as_ref()
            .is_some_and(|m| !m.closed && m.captures.contains_key("tag"))
    }));
    assert!(
        r.graph
            .syntax_coverage
            .values()
            .any(|s| s.partial_regions > 0)
    );
}
#[test]
fn dynamic_captures_and_header_spans_survive_warm_cache_and_edits() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-dynamic-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.tsx"), "const View = <Box title=\"a\" />;").unwrap();
    std::fs::write(root.join("b.tsx"), "const Other = <Other />;").unwrap();
    let indexer = Indexer::new(LanguageDefinition::load(Path::new("../../languages/tsx")).unwrap());
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
    assert!(warm.snapshot.graph.nodes.iter().any(|n| {
        n.lexical_region
            .as_ref()
            .is_some_and(|r| r.self_closed && r.header.is_some() && r.captures.contains_key("tag"))
    }));
    std::fs::write(
        root.join("a.tsx"),
        "const View = <Box title=\"changed\"><Leaf /></Box>;",
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
#[test]
fn ancestor_recovery_does_not_escape_a_protected_intermediate_context() {
    let mut d = serde_json::to_value(definition()).unwrap();
    d["regions"]["code"]["children"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            serde_json::json!({"pattern":"<(?P<tag>[A-Za-z][A-Za-z0-9_.:-]*)","region":"element"}),
        );
    let d = serde_json::from_value(d).unwrap();
    assert!(
        Lexer::new(&d)
            .unwrap()
            .tokenize("const V = <Outer>{<Inner>text</Outer>}more</Outer>; const after = 2;")
            .is_err()
    );
}
#[test]
fn explicitly_transparent_markup_contexts_can_recover_to_a_distant_ancestor() {
    let l = LanguageDefinition::load(Path::new("../../languages/tsx")).unwrap();
    let tokens = Lexer::new(&l.lexer)
        .unwrap()
        .tokenize("const V = <Outer><Middle><Inner>text</Outer>; const after = 2;")
        .unwrap();
    let t = tokens.iter().find(|t| t.kind == "jsx_atom").unwrap();
    let j = serde_json::to_value(t).unwrap();
    let r = j["regions"].as_array().unwrap();
    assert_eq!(r.len(), 3);
    assert_eq!(r[0]["closed"], true);
    assert_eq!(r[1]["closed"], false);
    assert_eq!(r[2]["closed"], false);
    assert!(tokens.iter().any(|t| t.text == "after"));
}

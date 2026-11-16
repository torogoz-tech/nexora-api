use nexora_core::RegionStatus;
use nexora_graph::NodeKind;
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn analyze(source: &str) -> CompositionResult {
    let model =
        LanguageDefinition::load(Path::new("../../templates/dotnet-understanding/csharp")).unwrap();
    CompositionAnalyzer::new(model, vec![])
        .unwrap()
        .analyze_source("source.cs", source)
        .unwrap()
}
#[test]
fn file_namespace_owns_types_with_opaque_headers_and_member_bodies() {
    let source = r#"global using System; namespace Demo.App;
[Tag("class Fake {}")] public partial class Box<T>(T value) : Base where T : new() { string s = $"{F("}")}"; }
internal interface IBox { void Run(); }
enum State : byte { Ready }
readonly record struct Point(int X, int Y);
record class Entry(string Name);
record Plain(int Value);
struct Value { int x; }"#;
    let result = analyze(source);
    let ast = &result.units[0].ast;
    for (kind, name) in [
        ("namespace", "Demo.App"),
        ("class", "Box"),
        ("interface", "IBox"),
        ("enum", "State"),
        ("record_struct", "Point"),
        ("record_class", "Entry"),
        ("record_class", "Plain"),
        ("struct", "Value"),
    ] {
        assert!(
            ast.nodes.iter().any(|n| n.kind == kind && n.name == name),
            "missing {kind} {name}"
        );
    }
    assert!(!ast.nodes.iter().any(|n| n.name == "Fake"));
    let class = ast.nodes.iter().find(|n| n.kind == "class").unwrap();
    let namespace = ast.nodes.iter().find(|n| n.kind == "namespace").unwrap();
    assert_eq!(class.parent_id.as_ref(), Some(&namespace.id));
    let body = class.regions.iter().find(|r| r.capture == "body").unwrap();
    assert_eq!(body.status, RegionStatus::Opaque);
    assert_eq!(
        &source[body.span.start.offset..body.span.end.offset],
        " string s = $\"{F(\"}\")}\"; "
    );
    assert!(!result.complete);
    for name in ["Point", "Value"] {
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.name == name && n.kind == NodeKind::Statement)
        );
        assert!(
            !result
                .graph
                .nodes
                .iter()
                .any(|n| n.name == name && n.kind == NodeKind::Class)
        );
    }
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
}
#[test]
fn block_namespaces_recurse_but_type_members_do_not_become_top_level_declarations() {
    let result = analyze(
        "namespace/* comment */Outer { namespace Inner { class/* comment */A { class Nested {} } } struct B {} } class After {}",
    );
    let nodes = &result.units[0].ast.nodes;
    for name in ["Outer", "Inner", "A", "B", "After"] {
        assert!(nodes.iter().any(|n| n.name == name), "missing {name}");
    }
    assert!(!nodes.iter().any(|n| n.name == "Nested"));
}
#[test]
fn unknown_statements_and_directives_preserve_known_siblings_without_claiming_conditional_declarations()
 {
    let result = analyze(
        "#nullable enable\nusing Alias = System.String; DoSomething(); class Before {}\n#if NEVER\nclass Conditional {}\n#endif\nclass UnknownAfter {}",
    );
    let nodes = &result.units[0].ast.nodes;
    assert!(
        nodes
            .iter()
            .any(|n| n.kind == "class" && n.name == "Before")
    );
    assert!(
        !nodes.iter().any(
            |n| n.kind == "class" && ["Conditional", "UnknownAfter"].contains(&n.name.as_str())
        )
    );
    assert!(!result.complete);
}

#[test]
fn unresolved_conditional_tail_shields_unbalanced_or_non_code_disabled_text() {
    let result = analyze(
        "class Before {}\n#if NEVER\n\"unterminated text } class Fake {\n#endif\nclass UnknownAfter {}",
    );
    let nodes = &result.units[0].ast.nodes;
    assert!(
        nodes
            .iter()
            .any(|n| n.kind == "class" && n.name == "Before")
    );
    assert_eq!(nodes.iter().filter(|n| n.kind == "class").count(), 1);
    assert!(!result.complete);
    let tail = result.units[0]
        .tokens
        .iter()
        .find(|t| t.kind == "conditional_directive")
        .unwrap();
    assert!(tail.regions[0].closed && tail.regions[0].closer.is_none());
}

#[test]
fn conditional_directives_with_all_preprocessor_whitespace_remain_opaque() {
    for whitespace in [
        "\t", "\u{b}", "\u{c}", "\u{a0}", "\u{1680}", "\u{2000}", "\u{202f}", "\u{3000}",
    ] {
        let source = format!(
            "class Before {{}}\n#{whitespace}if false\nclass Inactive {{}}\n#endif\nclass UnknownAfter {{}}"
        );
        let result = analyze(&source);
        let declarations: Vec<_> = result.units[0]
            .ast
            .nodes
            .iter()
            .filter(|n| n.kind == "class")
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(declarations, ["Before"], "whitespace {whitespace:?}");
        assert!(!result.complete);
    }
}

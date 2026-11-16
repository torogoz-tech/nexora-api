use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;
fn analyze(source: &str) -> CompositionResult {
    let root = Path::new("../../templates/dotnet-understanding");
    CompositionAnalyzer::new(
        LanguageDefinition::load(&root.join("razor")).unwrap(),
        vec![LanguageDefinition::load(&root.join("csharp")).unwrap()],
    )
    .unwrap()
    .analyze_source("public.razor", source)
    .unwrap()
}
fn contents<'a, 'b>(source: &'a str, result: &'b CompositionResult) -> Vec<(&'b str, &'a str)> {
    result
        .units
        .iter()
        .skip(1)
        .map(|u| {
            (
                u.context.as_deref().unwrap(),
                &source[u.span.start.offset..u.span.end.offset],
            )
        })
        .collect()
}
#[test]
fn root_branches_keep_conditions_and_markup_expressions_with_literal_braces_separate() {
    let source = "@if (first) { <p>@A</p> } \n else if (second) { <p>literal } @B</p> } else { <p>@C</p> } @(After())";
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (first) "),
            ("expression_opaque", "A"),
            ("control_header_opaque", " (second) "),
            ("expression_opaque", "B"),
            ("expression_opaque", "C"),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    assert!(!result.complete);
}
#[test]
fn nested_branches_and_comment_trivia_keep_following_siblings() {
    let source = "@foreach (var item in items) { if (first) { <p>@A</p> } /* } <Fake> */ else if (second) { <p>@B</p> } @* } *@ else { <p>@C</p> } <p>@D</p> } @(After())";
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (var item in items) "),
            ("control_header_opaque", " (first) "),
            ("expression_opaque", "A"),
            ("control_header_opaque", " (second) "),
            ("expression_opaque", "B"),
            ("expression_opaque", "C"),
            ("expression_opaque", "D"),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
}
#[test]
fn root_comment_regions_preserve_chain_context_without_losing_region_evidence() {
    for separator in [
        " @* else { fake } *@ ",
        " /* else { fake } */ ",
        " // else { fake }\n ",
    ] {
        let source =
            format!("@if (first) {{ <p>@A</p> }}{separator}else {{ <p>@B</p> }} @(After())");
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (first) "),
                ("expression_opaque", "A"),
                ("expression_opaque", "B"),
                ("expression_opaque", "After()")
            ]
        );
        let comment = result.units[0]
            .tokens
            .iter()
            .find(|t| {
                t.regions.iter().any(|r| {
                    matches!(
                        r.mode.as_str(),
                        "razor_comment"
                            | "c_block_comment"
                            | "c_line_comment"
                            | "branch_line_comment"
                            | "branch_block_comment"
                    )
                })
            })
            .unwrap();
        assert!(comment.trivia);
        assert!(comment.regions.iter().all(|r| r.closed));
    }
}
#[test]
fn branch_words_remain_literal_outside_control_context_and_after_markup_or_expressions() {
    for source in [
        "else { literal } elseif { literal }",
        "@foreach (var item in items) { <p>@A</p> } else { literal }",
        "@while (first) { <p>@A</p> } else { literal }",
        "@if (first) { <p>@A</p> } elsewhere { literal }",
        "@if (first) { <p>@A</p> } <div>else { literal }</div> else { literal }",
        "@(Value) else { literal }",
        "text // else { literal } /* literal */",
    ] {
        let result = analyze(source);
        assert!(result.units[0].tokens.iter().all(|t| !matches!(
            t.kind.as_str(),
            "branch_if" | "branch_else" | "branch_line_comment" | "branch_block_comment"
        )));
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}
#[test]
fn truncated_else_markup_does_not_fabricate_a_closed_body_or_call_targets() {
    let source = "@if (first) { <p>@A</p> } else { <div>literal } @(After())";
    let result = analyze(source);
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| !r.closed)
    );
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
    assert!(!result.complete);
}
#[test]
fn line_comments_and_markup_lines_end_only_at_real_line_terminators() {
    for newline in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
        let source = format!(
            "@if (first) {{ <p>@A</p> }} // else {{ fake }} with spaces{newline}else {{ <p>@B</p> }} @(After())"
        );
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (first) "),
                ("expression_opaque", "A"),
                ("expression_opaque", "B"),
                ("expression_opaque", "After()")
            ]
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        let source =
            format!("@if (first) {{ @:literal words }} @(Value()){newline}<p>@B</p> }} @(After())");
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (first) "),
                ("expression_opaque", "Value()"),
                ("expression_opaque", "B"),
                ("expression_opaque", "After()")
            ]
        );
        let line = result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .find(|r| r.mode == "body_line")
            .unwrap();
        assert_eq!(
            &source[line.content_span.start.offset..line.content_span.end.offset],
            "literal words } @(Value())"
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}
#[test]
fn carrier_branches_do_not_extract_razor_from_csharp_strings_or_plain_else_text() {
    for (prefix, suffix) in [("<div>", "</div>"), ("<!-- ", " -->"), ("@:", "\n")] {
        let source = format!(
            r#"@if (outer) {{ {prefix}@if (flag) {{ <p>@A</p> }} else {{ var s = "@(Fake())"; <p>@B</p> }}{suffix} }} @(After())"#
        );
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (outer) "),
                ("control_header_opaque", " (flag) "),
                ("expression_opaque", "A"),
                ("expression_opaque", "B"),
                ("expression_opaque", "After()")
            ]
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        let source = format!(
            r#"@if (outer) {{ {prefix}@if (flag) {{ <p>@A</p> }} else if (next) {{ var s = "@(Fake())"; <p>@B</p> }} else {{ <p>@C</p> }}{suffix} }} @(After())"#
        );
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (outer) "),
                ("control_header_opaque", " (flag) "),
                ("expression_opaque", "A"),
                ("control_header_opaque", " (next) "),
                ("expression_opaque", "B"),
                ("expression_opaque", "C"),
                ("expression_opaque", "After()")
            ]
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        let source = format!(
            "@if (outer) {{ {prefix}@if (flag) {{ <p>@A</p> }} intervening text else {{ literal @(Visible()) }}{suffix} }} @(After())"
        );
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (outer) "),
                ("control_header_opaque", " (flag) "),
                ("expression_opaque", "A"),
                ("expression_opaque", "Visible()"),
                ("expression_opaque", "After()")
            ]
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}
#[test]
fn unsupported_branch_separators_are_unknown_instead_of_literal_csharp_strings() {
    let source = r#"@if (first) { <p>@A</p> } else /* pending separator */ if (second) { var s="@(Fake())"; } @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (first) "),
            ("expression_opaque", "A")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| r.mode == "unknown_tail")
    );
    assert!(!result.complete);
}
#[test]
fn carrier_literal_gaps_interrupt_branch_context_without_fabricating_email_code() {
    for (prefix, suffix) in [("<div>", "</div>"), ("<!-- ", " -->"), ("@:", "\n")] {
        for (gap, expression) in [
            ("@@", None),
            ("user@example.org", None),
            ("éclair@example.org", None),
            ("99@example.org", None),
            ("@(Value())", Some("Value()")),
            ("@Name", Some("Name")),
            ("𐐀@Name", Some("Name")),
            ("<span>gap</span>", None),
        ] {
            let source = format!(
                "@if (outer) {{ {prefix}@if (flag) {{ <p>@A</p> }} {gap} else {{ literal @(Visible()) }}{suffix} }} @(After())"
            );
            let result = analyze(&source);
            let mut expected = vec![
                ("control_header_opaque", " (outer) "),
                ("control_header_opaque", " (flag) "),
                ("expression_opaque", "A"),
            ];
            if let Some(expression) = expression {
                expected.push(("expression_opaque", expression));
            }
            expected.extend([
                ("expression_opaque", "Visible()"),
                ("expression_opaque", "After()"),
            ]);
            assert_eq!(contents(&source, &result), expected);
            assert!(
                result.units[0]
                    .tokens
                    .iter()
                    .flat_map(|t| &t.regions)
                    .all(|r| r.closed)
            );
        }
    }
}
#[test]
fn comments_without_confirmed_branches_remain_opaque_and_keep_following_code() {
    for comment in ["/* @(MaybeMarkup()) */", "// @(MaybeMarkup())\n"] {
        let source = format!("@if (first) {{ <p>@A</p> }} {comment} @(After())");
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (first) "),
                ("expression_opaque", "A"),
                ("expression_opaque", "After()")
            ]
        );
        let comment = result.units[0]
            .ast
            .nodes
            .iter()
            .find(|n| {
                n.lexical_region.as_ref().is_some_and(|r| {
                    matches!(
                        r.mode.as_str(),
                        "branch_line_comment" | "branch_block_comment"
                    )
                })
            })
            .unwrap();
        assert!(
            comment
                .regions
                .iter()
                .any(|r| r.status == nexora_core::RegionStatus::Opaque)
        );
        assert!(!result.complete);
    }
}
#[test]
fn multiline_branch_separators_are_supported_in_root_code_and_non_line_carriers() {
    for (prefix, suffix) in [
        ("", ""),
        ("@foreach (var item in items) { ", " }"),
        ("@if (outer) { <div>", "</div> }"),
        ("<!-- ", " -->"),
    ] {
        for newline in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
            let transition = if prefix.starts_with("@foreach") {
                ""
            } else {
                "@"
            };
            let source = format!(
                "{prefix}{transition}if (first) {{ <p>@A</p> }} else{newline}if (second) {{ <p>@B</p> }} else{newline}{{ var s=\"@(Fake())\"; <p>@C</p> }}{suffix} @(After())"
            );
            let result = analyze(&source);
            let mut expected = vec![];
            if prefix.starts_with("@foreach") {
                expected.push(("control_header_opaque", " (var item in items) "));
            }
            if prefix.starts_with("@if") {
                expected.push(("control_header_opaque", " (outer) "));
            }
            expected.extend([
                ("control_header_opaque", " (first) "),
                ("expression_opaque", "A"),
                ("control_header_opaque", " (second) "),
                ("expression_opaque", "B"),
                ("expression_opaque", "C"),
                ("expression_opaque", "After()"),
            ]);
            assert_eq!(contents(&source, &result), expected);
            assert!(
                result.units[0]
                    .tokens
                    .iter()
                    .flat_map(|t| &t.regions)
                    .all(|r| r.closed)
            );
        }
    }
}
#[test]
fn line_carrier_comments_leave_their_terminator_for_the_parent() {
    for newline in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
        for comment in ["// comment", "/* comment */"] {
            let source = format!(
                "@if (outer) {{ @:@if (flag) {{ <p>@A</p> }} {comment}{newline} var s=\"@(Fake())\"; <p>@B</p> }} @(After())"
            );
            let result = analyze(&source);
            assert_eq!(
                contents(&source, &result),
                [
                    ("control_header_opaque", " (outer) "),
                    ("control_header_opaque", " (flag) "),
                    ("expression_opaque", "A"),
                    ("expression_opaque", "B"),
                    ("expression_opaque", "After()")
                ]
            );
            assert!(
                result.units[0]
                    .tokens
                    .iter()
                    .flat_map(|t| &t.regions)
                    .all(|r| r.closed)
            );
        }
    }
}
#[test]
fn multiline_block_comments_in_a_line_carrier_are_explicitly_unknown() {
    let source = "@if (outer) { @:@if (flag) { <p>@A</p> } /* comment\ncontinued */ var s=\"@(Fake())\"; <p>@B</p> } @(After())";
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (outer) "),
            ("control_header_opaque", " (flag) "),
            ("expression_opaque", "A")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| r.mode == "unknown_tail")
    );
    assert!(!result.complete);
}

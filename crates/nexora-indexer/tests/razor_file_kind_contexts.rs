use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn analyze(host: &str, provider: &str, extension: &str, source: &str) -> CompositionResult {
    let root = Path::new("../../templates/dotnet-understanding");
    CompositionAnalyzer::new(
        LanguageDefinition::load(&root.join(host)).unwrap(),
        vec![LanguageDefinition::load(&root.join(provider)).unwrap()],
    )
    .unwrap()
    .analyze_source(&format!("public.{extension}"), source)
    .unwrap()
}

#[test]
fn mvc_only_directive_words_are_ordinary_component_expression_names() {
    for name in [
        "model",
        "addTagHelper",
        "removeTagHelper",
        "tagHelperPrefix",
    ] {
        for source in [
            format!("@if(Ready){{@{name}}} @(After())"),
            format!("@{{ @{name} }} @(After())"),
        ] {
            let result = analyze("razor", "csharp", "razor", &source);
            assert!(
                result
                    .units
                    .iter()
                    .any(|u| u.context.as_deref() == Some("expression_opaque")
                        && &source[u.span.start.offset..u.span.end.offset] == name),
                "{name}"
            );
            assert!(
                result
                    .units
                    .iter()
                    .flat_map(|u| &u.tokens)
                    .flat_map(|t| &t.regions)
                    .all(|r| r.closed)
            );
        }
    }
}

#[test]
fn catalog_selects_file_kinds_without_changing_the_pure_csharp_provider() {
    let registry =
        nexora_language::LanguageRegistry::load(Path::new("../../templates/dotnet-understanding"))
            .unwrap();
    for (file, language) in [
        ("public.razor", "razor"),
        ("public.cshtml", "razor_views"),
        ("public.cs", "csharp"),
    ] {
        assert_eq!(
            registry.detect(Path::new(file)).unwrap().manifest.id.0,
            language
        );
        assert_eq!(registry.candidates(Path::new(file)).len(), 1);
    }
    let dependencies = registry
        .composition_dependencies(registry.get("razor_views").unwrap())
        .unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].manifest.id.0, "csharp_views");
    assert_eq!(
        registry
            .composition_dependencies(registry.get("razor").unwrap())
            .unwrap()[0]
            .manifest
            .id
            .0,
        "csharp"
    );
}

#[test]
fn views_keep_their_own_directives_and_body_context_in_host_and_provider() {
    for name in [
        "model",
        "addTagHelper",
        "removeTagHelper",
        "tagHelperPrefix",
    ] {
        let source = format!("@{{ @{name} }} @(After())");
        let result = analyze("razor_views", "csharp_views", "cshtml", &source);
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .any(|r| r.mode == "unknown_tail")
        );
        assert!(
            result
                .units
                .iter()
                .all(|u| u.context.as_deref() != Some("expression_opaque")
                    || &source[u.span.start.offset..u.span.end.offset] != name)
        );
    }
    let source = "@model PublicModel\n@if(Ready){@Value} @(After())";
    let result = analyze("razor_views", "csharp_views", "cshtml", source);
    assert!(
        result
            .units
            .iter()
            .flat_map(|u| &u.tokens)
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    assert_eq!(
        result
            .units
            .iter()
            .filter(|u| u.context.as_deref() == Some("expression_opaque"))
            .map(|u| &source[u.span.start.offset..u.span.end.offset])
            .collect::<Vec<_>>(),
        ["Value", "After()"]
    );
    assert!(
        result
            .units
            .iter()
            .skip(1)
            .all(|u| u.language.0 == "csharp_views")
    );
}

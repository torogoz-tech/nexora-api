use nexora_mcp::ToolCatalog;
use serde_json::json;
#[test]
fn read_catalog_includes_knowledge_and_pending_without_granting_write_access() {
    let tools = ToolCatalog::list(false);
    for name in [
        "nexora_list_pending",
        "nexora_list_knowledge",
        "nexora_get_knowledge",
    ] {
        let tool = tools
            .iter()
            .find(|t| t["name"] == name)
            .expect("knowledge read tools missing");
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
    }
    assert!(ToolCatalog::validate("nexora_contribute_knowledge", &json!({}), true).is_err());
}
#[test]
fn write_catalog_is_explicit_and_nested_payloads_are_strict() {
    let tools = ToolCatalog::list_with_knowledge(false, true);
    assert_eq!(tools.len(), 13);
    assert!(!tools.iter().any(|t| t["name"] == "nexora_index_repository"));
    let tool = tools
        .iter()
        .find(|t| t["name"] == "nexora_contribute_knowledge")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    let valid = json!({"request_id":"request-1","draft":{"kind":"decision","statement":"Reviewed decision","evidence":[{"path":"a.js","start_line":1,"line_count":1}]}});
    assert!(
        ToolCatalog::validate_with_knowledge("nexora_contribute_knowledge", &valid, false, true)
            .is_ok()
    );
    assert!(
        ToolCatalog::validate_with_knowledge("nexora_contribute_knowledge", &valid, true, false)
            .is_err()
    );
    for replacement in [
        json!([]),
        json!([{"path":"a.js","start_line":1,"line_count":1,"tenant":"other"}]),
        json!([{"path":"a.js","start_line":0,"line_count":1}]),
        json!([{"path":"../secret","start_line":1,"line_count":1}]),
    ] {
        let mut invalid = valid.clone();
        invalid["draft"]["evidence"] = replacement;
        assert!(
            ToolCatalog::validate_with_knowledge(
                "nexora_contribute_knowledge",
                &invalid,
                false,
                true
            )
            .is_err()
        );
    }
    assert!(
        ToolCatalog::validate_with_knowledge(
            "nexora_list_knowledge",
            &json!({"status":"invented"}),
            false,
            false
        )
        .is_err()
    );
    assert!(
        ToolCatalog::validate_with_knowledge(
            "nexora_get_knowledge",
            &json!({"id":"k","tenant":"other"}),
            false,
            false
        )
        .is_err()
    );
}

#[test]
fn optional_fields_returned_by_notes_can_be_sent_back_for_revision() {
    let args = json!({"id":"knowledge:example","request_id":"note-revision","expected_revision":1,"draft":{"kind":"explanation","statement":"Clarified explanation","source":null,"target":null,"relation":null,"evidence":[{"path":"a.js","start_line":1,"line_count":1}]}});
    assert!(
        ToolCatalog::validate_with_knowledge("nexora_revise_knowledge", &args, false, true).is_ok()
    );
    let mut invalid = args;
    invalid["draft"]["kind"] = json!("relation");
    assert!(
        ToolCatalog::validate_with_knowledge("nexora_revise_knowledge", &invalid, false, true)
            .is_err()
    );
}

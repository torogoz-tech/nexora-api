use crate::document_mcp_catalog::DocumentMcpCatalog;

#[test]
fn knowledge_tools_do_not_request_or_advertise_workflows() {
    let tools = DocumentMcpCatalog::list();
    assert!(
        !tools
            .iter()
            .any(|tool| tool["name"] == "nexora_document_transition")
    );
    assert!(!DocumentMcpCatalog::supports("nexora_document_transition"));
    for name in ["nexora_document_create", "nexora_document_revise"] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        let definitions = &tool["inputSchema"]["$defs"];
        let definition = if name.ends_with("create") {
            "ProjectDocumentCreate"
        } else {
            "ProjectDocumentRevise"
        };
        assert!(
            definitions[definition]["properties"]
                .get("workflow_version_id")
                .is_none()
        );
        assert!(
            !definitions[definition]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "workflow_version_id")
        );
    }
}

use super::AnalysisProfileAuthoring;
use crate::{
    analysis_model_guide::AnalysisModelGuide,
    mcp_analysis_model_guide_get::McpAnalysisModelGuideGet,
    mcp_analysis_profile_package::McpAnalysisProfilePackage,
};
use nexora_language::AnalysisProfilePackage;
use serde_json::json;
use std::path::Path;

#[test]
fn all_existing_models_package_with_the_importer_canonical_checksum() {
    for language in [
        "javascript",
        "typescript",
        "jsx",
        "tsx",
        "html",
        "css",
        "lua",
    ] {
        let pack = AnalysisProfilePackage::from_catalog(
            Path::new("../../languages"),
            language.into(),
            format!("team-{language}"),
            "1.0.0".into(),
            language.into(),
        )
        .unwrap();
        let output = AnalysisProfileAuthoring::package(McpAnalysisProfilePackage {
            key: pack.key.clone(),
            version: pack.version.clone(),
            name: pack.name.clone(),
            language_id: pack.language_id.clone(),
            models: pack.models.clone(),
        })
        .unwrap();
        assert_eq!(output["valid"], true, "{language}: {output}");
        assert_eq!(output["sha256"], pack.checksum().unwrap());
        assert_eq!(output["semantic_coverage_verified"], false);
    }
}
#[test]
fn composed_model_dependencies_and_unknown_fields_cannot_be_omitted() {
    let mut pack = AnalysisProfilePackage::from_catalog(
        Path::new("../../templates/composition-markup"),
        "region_carrier".into(),
        "composed".into(),
        "1.0.0".into(),
        "Composed".into(),
    )
    .unwrap();
    assert_eq!(
        AnalysisProfileAuthoring::validate(serde_json::to_value(&pack).unwrap())["valid"],
        true
    );
    pack.models
        .retain(|model| model.manifest.id.0 == "region_carrier");
    let invalid = AnalysisProfileAuthoring::validate(serde_json::to_value(&pack).unwrap());
    assert_eq!(invalid["valid"], false);
    assert!(!invalid["errors"][0].as_str().unwrap().is_empty());
    let bad = AnalysisProfileAuthoring::validate(
        json!({"format":"nexora.analysis-profile","directory":"/etc/nexora"}),
    );
    assert_eq!(bad["valid"], false);
}
#[test]
fn every_advertised_schema_is_bundled_and_unknown_versions_fail_closed() {
    let guide = AnalysisModelGuide::get(McpAnalysisModelGuideGet {
        component: None,
        version: None,
    })
    .unwrap();
    for component in guide["components"].as_array().unwrap() {
        for version in component["versions"].as_array().unwrap() {
            let result = AnalysisModelGuide::get(McpAnalysisModelGuideGet {
                component: Some(component["name"].as_str().unwrap().into()),
                version: Some(version.as_u64().unwrap() as u32),
            })
            .unwrap();
            assert!(result["schema"]["properties"].is_object());
        }
    }
    for input in [
        McpAnalysisModelGuideGet {
            component: Some("grammar".into()),
            version: Some(1),
        },
        McpAnalysisModelGuideGet {
            component: Some("lexer".into()),
            version: Some(99),
        },
        McpAnalysisModelGuideGet {
            component: Some("/etc/passwd".into()),
            version: None,
        },
        McpAnalysisModelGuideGet {
            component: None,
            version: Some(1),
        },
    ] {
        assert!(AnalysisModelGuide::get(input).is_err());
    }
}

#[test]
#[ignore = "requires PostgreSQL; stale preauthenticated contexts cannot invoke authoring"]
fn authoring_rechecks_a_credential_revoked_after_context_creation() {
    use nexora_platform::{Capability, KeyScope};
    use nexora_platform_store::{PlatformStore, SecretGenerator};
    use nexora_runtime::DatabaseConnections;
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_stale_author").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Stale authoring", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let owner = store.authenticate(&session.access_token).unwrap();
    let key = store
        .issue_key(
            &owner,
            owner.actor_id(),
            "Author",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![],
                all_projects: true,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let stale = store.authenticate(&key.key).unwrap();
    store.revoke_key(&owner, &key.metadata.id).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let state = runtime
        .block_on(crate::ApiState::new(config, namespace, 2))
        .unwrap();
    for (name, args) in [
        ("nexora_analysis_model_guide_get", json!({})),
        ("nexora_analysis_profile_validate", json!({"package":{}})),
        ("nexora_analysis_profile_package", json!({})),
    ] {
        let output = runtime
            .block_on(
                crate::analysis_profile_authoring_mcp_tools::AnalysisProfileAuthoringMcpTools::call(
                    state.clone(),
                    stale.clone(),
                    name,
                    args,
                    true,
                ),
            )
            .unwrap();
        assert_eq!(output["isError"], true, "{output}");
        assert!(output.get("structuredContent").is_none());
        assert_eq!(
            output["content"][0]["text"],
            nexora_platform::PlatformError::Unauthorized.to_string()
        );
    }
}

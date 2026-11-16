use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_language::AnalysisProfilePackage;
use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::path::Path;
use tower::ServiceExt;

async fn call(app: &axum::Router, token: &str, name: &str, args: Value) -> (StatusCode, Value) {
    call_version(app, token, name, args, "2025-11-25").await
}
async fn call_version(
    app: &axum::Router,
    token: &str,
    name: &str,
    args: Value,
    protocol: &str,
) -> (StatusCode, Value) {
    let response = app.clone().oneshot(Request::post("/mcp")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", protocol)
        .body(Body::from(serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.len() <= 1_048_576);
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[test]
#[ignore = "requires PostgreSQL; developer MCP authoring never imports or activates models"]
fn developer_can_author_validate_and_share_without_administrative_access() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_authoring").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Authoring", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let owner = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&owner, "Team", None).unwrap();
    let agent = store
        .create_actor(
            &owner,
            ActorKind::Agent,
            "Developer agent",
            "Model authoring",
        )
        .unwrap();
    store
        .grant(
            &owner,
            &agent.id,
            Capability::ProjectsRead,
            Some(&project.id),
        )
        .unwrap();
    let key = store
        .issue_key(
            &owner,
            &agent.id,
            "Scoped developer",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.id],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let pack = AnalysisProfilePackage::from_catalog(
        Path::new("../../languages/lua"),
        "lua".into(),
        "team-lua".into(),
        "1.0.0".into(),
        "Team Lua".into(),
    )
    .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = ApiServer::router(
        runtime
            .block_on(ApiState::new(config, namespace, 4))
            .unwrap(),
    );
    runtime.block_on(async {
        let catalog_response=app.clone().oneshot(Request::post("/mcp")
            .header("Authorization",format!("Bearer {}",key.key))
            .header("Content-Type","application/json")
            .header("Accept","application/json, text/event-stream")
            .header("MCP-Protocol-Version","2025-11-25")
            .body(Body::from(serde_json::to_vec(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap())).unwrap()).await.unwrap();
        let catalog:Value=serde_json::from_slice(&catalog_response.into_body().collect().await.unwrap().to_bytes()).unwrap();
        for name in ["nexora_analysis_model_guide_get","nexora_analysis_profile_validate","nexora_analysis_profile_package"] {
            let tool=catalog["result"]["tools"].as_array().unwrap().iter().find(|tool|tool["name"]==name).unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"],true);
            assert_eq!(tool["inputSchema"]["additionalProperties"],false);
        }
        let (_, guide) = call(&app, &key.key, "nexora_analysis_model_guide_get", json!({})).await;
        assert_eq!(guide["result"]["isError"], false, "{guide}");
        for (component, version) in [("language",1),("lexer",14),("grammar",10),("graph",10)] {
            let (_, schema) = call(&app,&key.key,"nexora_analysis_model_guide_get",json!({"component":component,"version":version})).await;
            assert_eq!(schema["result"]["isError"], false, "{schema}");
            assert!(schema["result"]["structuredContent"]["schema"].is_object());
        }
        let (_, bad) = call(&app,&key.key,"nexora_analysis_model_guide_get",json!({"component":"../../secret","version":1})).await;
        assert_eq!(bad["result"]["isError"], true);
        let (_, valid) = call(&app,&key.key,"nexora_analysis_profile_validate",json!({"package":pack})).await;
        assert_eq!(valid["result"]["structuredContent"]["valid"], true, "{valid}");
        let (_, generated) = call(&app,&key.key,"nexora_analysis_profile_package",json!({"key":pack.key,"version":pack.version,"name":pack.name,"language_id":pack.language_id,"models":pack.models})).await;
        assert_eq!(generated["result"]["isError"], false, "{generated}");
        let result = &generated["result"]["structuredContent"];
        assert_eq!(result["sha256"], pack.checksum().unwrap());
        let exported: AnalysisProfilePackage = serde_json::from_str(result["json"].as_str().unwrap()).unwrap();
        assert_eq!(exported.checksum().unwrap(), pack.checksum().unwrap());
        assert!(result["file_name"].as_str().unwrap().ends_with(".nexora-profile.json"));
        let (_, legacy) = call_version(&app,&key.key,"nexora_analysis_model_guide_get",json!({"component":"language","version":1}),"2025-03-26").await;
        assert_eq!(legacy["result"]["isError"],false);
        assert!(legacy["result"]["structuredContent"].is_null());
        let legacy_value:Value=serde_json::from_str(legacy["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(legacy_value["schema"].is_object());
        // Valid near-budget rules exercise actual transport envelopes, including
        // JSON escaping that makes structured+text output much larger than input.
        for (pattern,count,expected_success) in [("a".repeat(4000),60,true),("\"".repeat(4000),31,false)] {
            let mut near = pack.clone();
            for index in 0..count {
                let mut token=near.models[0].lexer.tokens[0].clone();
                token.kind=format!("BOUNDARY_{index}");token.pattern=pattern.clone();
                near.models[0].lexer.tokens.push(token);
            }
            near.validate().unwrap();
            let args=json!({"key":near.key,"version":near.version,"name":near.name,"language_id":near.language_id,"models":near.models});
            let size=serde_json::to_vec(&args).unwrap().len();
            assert!(size>235_000 && size<=262_144,"near-budget size: {size}");
            let (_, result)=call(&app,&key.key,"nexora_analysis_profile_package",args.clone()).await;
            assert_eq!(result["result"]["isError"],!expected_success,"{result}");
            if expected_success { assert_eq!(result["result"]["structuredContent"]["valid"],true); }
            else { assert!(result["result"]["content"][0]["text"].as_str().unwrap().contains("response budget")); }
            let (_, legacy)=call_version(&app,&key.key,"nexora_analysis_profile_package",args,"2025-03-26").await;
            assert_eq!(legacy["result"]["isError"],!expected_success,"{legacy}");
        }
        let mut malformed = serde_json::to_value(&pack).unwrap();
        malformed["models"][0]["lexer"]["tokens"][0]["pattern"] = json!("");
        let (_, invalid) = call(&app,&key.key,"nexora_analysis_profile_validate",json!({"package":malformed})).await;
        assert_eq!(invalid["result"]["structuredContent"]["valid"], false);
        assert!(invalid["result"]["structuredContent"]["errors"][0].as_str().unwrap().len()>0);
        let (_, oversized) = call(&app,&key.key,"nexora_analysis_profile_package",json!({"key":"large","version":"1.0.0","name":"x".repeat(270_000),"language_id":"lua","models":pack.models})).await;
        assert_eq!(oversized["result"]["isError"], true);
        let denied = app.clone().oneshot(Request::post("/api/v1/analysis-profiles")
            .header("Authorization",format!("Bearer {}",key.key)).header("Content-Type","application/json")
            .header("Idempotency-Key","authoring-cannot-install")
            .body(Body::from(serde_json::to_vec(&pack).unwrap())).unwrap()).await.unwrap();
        assert_eq!(denied.status(),StatusCode::FORBIDDEN);
    });
    assert!(
        store.analysis_profiles(&owner, None, 100).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    store.revoke_key(&owner, &key.metadata.id).unwrap();
    let (status, _) = runtime.block_on(call(
        &app,
        &key.key,
        "nexora_analysis_model_guide_get",
        json!({}),
    ));
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

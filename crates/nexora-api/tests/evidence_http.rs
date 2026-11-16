#[path = "../../nexora-platform-store/tests/support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::BodyExt;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::{Capability, KeyScope};
use serde_json::{Value, json};
use tower::ServiceExt;
#[test]
#[ignore = "requires local databases; evidence bytes, receipts and authority"]
fn evidence_is_durable_idempotent_scoped_and_rendered_by_mcp() {
    let mut f = LocalSnapshotFixture::new();
    let published = f.adopt();
    let project = f.work.project.id.clone();
    let repository = published.repository_id.clone();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Evidence fixture",
            KeyScope {
                capabilities: vec![
                    Capability::RepositoriesRead,
                    Capability::KnowledgeRead,
                    Capability::KnowledgeWrite,
                ],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let call = |name: &str, args: Value| {
        runtime.block_on(async{
        let request=Request::builder().method("POST").uri("/mcp").header("authorization",format!("Bearer {}",key.key)).header("content-type","application/json").header("accept","application/json, text/event-stream").header("mcp-protocol-version","2025-11-25").body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string())).unwrap();
        let response=app.clone().oneshot(request).await.unwrap();let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();(status,serde_json::from_slice::<Value>(&bytes).unwrap())
    })
    };
    runtime.block_on(async {
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("authorization", format!("Bearer {}", key.key))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .body(Body::from(
                    json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}).to_string(),
                ))
                .unwrap()
        };
        let (one, two, three) = tokio::join!(
            app.clone().oneshot(request()),
            app.clone().oneshot(request()),
            app.clone().oneshot(request())
        );
        for response in [one, two, three] {
            assert_eq!(
                response.unwrap().status(),
                StatusCode::OK,
                "bounded waiting must admit a short concurrent burst"
            );
        }
    });
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aC9sAAAAASUVORK5CYII=";
    let input = json!({"repository_id":repository,"name":"after.png","media_type":"image/png","data_base64":png,"phase":"after","snapshot":f.header.id.0,"description":"Provided screenshot","provided_metadata":{"viewport":{"width":1440,"height":900,"dpr":1}}});
    let args = json!({"project_id":project,"request_id":"upload-evidence-1","evidence":input});
    let first = call("nexora_evidence_upload", args.clone());
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.1["result"]["isError"], false, "{first:?}");
    let record = first.1["result"]["structuredContent"].clone();
    let id = record["id"].as_str().unwrap();
    assert_eq!(record["created_by"], f.work.ctx.actor_id());
    assert_eq!(
        call("nexora_evidence_upload", args.clone()).1["result"]["structuredContent"],
        record
    );
    let mut different = args;
    different["evidence"]["description"] = json!("Changed meaning");
    assert_eq!(
        call("nexora_evidence_upload", different).1["result"]["isError"],
        true
    );
    let fetched = call(
        "nexora_evidence_get",
        json!({"project_id":project,"evidence_id":id,"include_content":true}),
    );
    assert_eq!(fetched.1["result"]["content"][1]["type"], "image");
    assert_eq!(fetched.1["result"]["content"][1]["data"], png);
    let foreign = call(
        "nexora_evidence_get",
        json!({"project_id":"foreign-project","evidence_id":id,"include_content":true}),
    );
    assert_eq!(foreign.1["result"]["isError"], true);
    // The MCP route accepts a bounded file above the old global1MiB JSON limit.
    let mut large_png = STANDARD.decode(png).unwrap();
    let ending = large_png.split_off(large_png.len() - 12);
    let mut chunk = b"tEXt".to_vec();
    chunk.extend_from_slice(b"Note\0");
    chunk.extend(vec![b'x'; 900000]);
    let crc = chunk.iter().fold(!0u32, |crc, byte| {
        (0..8).fold(crc ^ u32::from(*byte), |crc, _| {
            if crc & 1 != 0 {
                0xedb88320 ^ (crc >> 1)
            } else {
                crc >> 1
            }
        })
    }) ^ !0u32;
    large_png.extend_from_slice(&((chunk.len() - 4) as u32).to_be_bytes());
    large_png.extend(chunk);
    large_png.extend(crc.to_be_bytes());
    large_png.extend(ending);
    let large = STANDARD.encode(large_png);
    let large_args = json!({"project_id":project,"request_id":"upload-evidence-large","evidence":{"repository_id":repository,"name":"large.png","media_type":"image/png","data_base64":large,"phase":"supporting","description":"Bounded PNG"}});
    let large_result = call("nexora_evidence_upload", large_args);
    assert_eq!(large_result.0, StatusCode::OK);
    assert_eq!(
        large_result.1["result"]["isError"], false,
        "{large_result:?}"
    );
    let large_id = large_result.1["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let large_read = call(
        "nexora_evidence_get",
        json!({"project_id":project,"evidence_id":large_id,"include_content":true}),
    );
    assert_eq!(large_read.1["result"]["content"][1]["data"], large);
    let listed = call(
        "nexora_evidence_list",
        json!({"project_id":project,"repository_id":repository,"limit":10}),
    );
    assert_eq!(
        listed.1["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let before = f
        .runtime
        .query_header(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap();
    assert_eq!(before.payload_hash, f.header.payload_hash);
    let count: i64 = f
        .sql
        .query_one("SELECT count(*) FROM evidence_files", &[])
        .unwrap()
        .get(0);
    assert_eq!(count, 2);
    let corrupt = vec![b'!'];
    f.sql
        .execute(
            "UPDATE evidence_files SET payload=$1 WHERE id=$2",
            &[&corrupt, &id],
        )
        .unwrap();
    assert_eq!(
        call(
            "nexora_evidence_get",
            json!({"project_id":project,"evidence_id":id})
        )
        .1["result"]["isError"],
        true
    );
    let change = nexora_platform::LocalSourceChange {
        expected_revision: published.source_revision,
        reason: "Revoke source-bound evidence".into(),
    };
    let operator = f.work.ctx.clone();
    f.work
        .store
        .request(
            &operator,
            "local_source.revoke",
            "evidence-revoke-source",
            &json!({"project_id":project,"repository_id":repository,"change":change}),
            |store| store.revoke_local_source(&operator, &project, &repository, &change),
        )
        .unwrap();
    let after_revoke = call(
        "nexora_evidence_list",
        json!({"project_id":project,"repository_id":repository,"limit":10}),
    );
    let visible = after_revoke.1["result"]["structuredContent"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(visible.len(), 1);
    assert!(visible.iter().all(|r| r["id"] != id));
    f.work
        .store
        .revoke_key(&f.work.ctx, &key.metadata.id)
        .unwrap();
    assert_eq!(
        call(
            "nexora_evidence_list",
            json!({"project_id":project,"repository_id":repository})
        )
        .0,
        StatusCode::UNAUTHORIZED
    );
}

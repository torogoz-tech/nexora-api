#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn typed_practices_and_debt_keep_governed_publication_and_history() {
    let mut f = WorkItemFixture::new();
    for (kind, body) in [
        (
            "best_practice",
            json!({"guideline":"Validate inputs","rationale":"Keep boundaries explicit","examples":["Reject unknown fields"],"source":"Team review"}),
        ),
        (
            "technical_debt",
            json!({"problem":"Legacy parser","impact":"Slow upgrades","remediation":"Replace incrementally","source":"Architecture review"}),
        ),
    ] {
        let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":kind,"purpose":"Document governance","resource_kinds":[kind],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"approved","name":"Approved","terminal":true,"publishable":true}],"transitions":[{"id":"approve","name":"Approve","from":"draft","to":"approved","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
        let flow = f
            .store
            .publish_workflow(
                &f.ctx,
                &f.project.id,
                &workflow.id,
                &WorkflowPublish {
                    expected_revision: workflow.revision,
                    reason: "Configure publication".into(),
                },
            )
            .unwrap();
        let actor = f
            .store
            .create_actor(
                &f.ctx,
                ActorKind::Agent,
                "MCP contributor",
                "Typed documentation",
            )
            .unwrap();
        let capabilities = vec![
            Capability::DocumentsRead,
            Capability::DocumentsWrite,
            Capability::DocumentsPublish,
        ];
        for capability in &capabilities {
            f.store
                .grant(&f.ctx, &actor.id, *capability, Some(&f.project.id))
                .unwrap();
        }
        let key = f
            .store
            .issue_key(
                &f.ctx,
                &actor.id,
                "Documentation",
                KeyScope {
                    capabilities,
                    project_ids: vec![f.project.id.clone()],
                    all_projects: false,
                    installation: false,
                },
                3600,
            )
            .unwrap();
        let agent = f.store.authenticate(&key.key).unwrap();
        let content = json!({"kind":kind,"body":body});
        let document=f.store.create_project_document(&agent,&f.project.id,&serde_json::from_value(json!({"code":format!("DOC-{kind}"),"title":kind,"content":content,"origin":"agent_proposed","workflow_version_id":flow.id,"reason":"MCP contribution"})).unwrap()).unwrap();
        let version = f
            .store
            .document_version(
                &agent,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
            )
            .unwrap();
        assert_eq!(serde_json::to_value(&version.content).unwrap(), content);
        assert!(f.store.publish_project_document(&agent,&f.project.id,&document.id,&serde_json::from_value(json!({"expected_revision":document.revision,"version_id":version.id,"expected_state_revision":version.state_revision,"reason":"Cannot skip approval"})).unwrap()).is_err());
        let approved = f
            .store
            .transition_document_version(
                &agent,
                &f.project.id,
                &document.id,
                &version.id,
                "approve",
                &serde_json::from_value(
                    json!({"expected_state_revision":version.state_revision,"reason":"Reviewed"}),
                )
                .unwrap(),
            )
            .unwrap();
        let publication=f.store.publish_project_document(&agent,&f.project.id,&document.id,&serde_json::from_value(json!({"expected_revision":document.revision,"version_id":version.id,"expected_state_revision":approved.state_revision,"reason":"Publish governed content"})).unwrap()).unwrap();
        assert_eq!(publication.version_id, version.id);
        let outsider = f
            .store
            .create_actor(
                &f.ctx,
                ActorKind::Agent,
                "No grant",
                "Permission regression",
            )
            .unwrap();
        let key = f.store.issue_key(
            &f.ctx,
            &outsider.id,
            "No document access",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        );
        assert!(key.is_err());
    }
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    drop(f.store);
    sql.batch_execute(&format!(r#"DROP SCHEMA "{}" CASCADE"#, f.namespace))
        .unwrap();
    std::fs::remove_file(
        f.config
            .secrets_directory
            .join(format!("{}_master_key_v1", f.namespace)),
    )
    .unwrap();
}

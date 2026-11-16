use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::path::Path;
use zeroize::Zeroizing;

#[allow(dead_code)]
pub struct WorkItemFixture {
    pub store: PlatformStore,
    pub config: DatabaseConnections,
    pub namespace: String,
    pub ctx: AuthContext,
    pub project: Project,
    pub backlog: Backlog,
}
#[allow(dead_code)]
impl WorkItemFixture {
    pub fn new() -> Self {
        let config =
            DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
        let namespace = SecretGenerator::id("nexora_platform_relations").unwrap();
        let mut store = PlatformStore::connect(&config, &namespace).unwrap();
        store.initialize().unwrap();
        let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
        store
            .bootstrap("Work fixture", "owner", &password, "Owner")
            .unwrap();
        let session = store.login("owner", &password).unwrap();
        let verified = store
            .identity_gate(&session.access_token, "skip", "")
            .unwrap();
        let ctx = store
            .authenticate(verified["access_token"].as_str().unwrap())
            .unwrap();
        let project = store.create_project(&ctx, "Project work", None).unwrap();
        let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Delivery","purpose":"Explicit dependency conditions","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"dependencies_satisfied"}]}]}})).unwrap()).unwrap();
        let version = store
            .publish_workflow(
                &ctx,
                &project.id,
                &workflow.id,
                &WorkflowPublish {
                    expected_revision: workflow.revision,
                    reason: "Fixture policy".into(),
                },
            )
            .unwrap();
        store
            .create_task_type(
                &ctx,
                &project.id,
                &TaskTypeCreate {
                    purpose: nexora_platform::TaskPurpose::General,
                    code: "task".into(),
                    name: "Task".into(),
                    description: None,
                    color: None,
                },
            )
            .unwrap();
        let backlog = store
            .create_backlog(
                &ctx,
                &project.id,
                &BacklogCreate {
                    name: "Delivery".into(),
                    description: None,
                    workflow_version_id: version.id,
                    color: None,
                },
            )
            .unwrap();
        Self {
            store,
            config,
            namespace,
            ctx,
            project,
            backlog,
        }
    }
    pub fn task(&mut self, title: &str) -> WorkTask {
        self.store
            .create_task(
                &self.ctx,
                &self.project.id,
                &TaskCreate {
                    title: title.into(),
                    type_code: "task".into(),
                    backlog_id: Some(self.backlog.id.clone()),
                    origin: "Fixture".into(),
                    ..Default::default()
                },
            )
            .unwrap()
    }
}

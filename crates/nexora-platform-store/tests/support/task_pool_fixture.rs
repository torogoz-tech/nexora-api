#[path = "work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[allow(dead_code)]
pub struct TaskPoolFixture {
    pub work: WorkItemFixture,
    pub developer: AuthContext,
    pub other: AuthContext,
    pub manager: AuthContext,
    pub version: WorkflowVersion,
}
#[allow(dead_code)]
impl TaskPoolFixture {
    pub fn new(approval: bool) -> Self {
        let mut work = WorkItemFixture::new();
        let password = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
        let mut contexts = Vec::new();
        for (name, role) in [
            ("developer", MemberRole::Developer),
            ("other", MemberRole::TechnicalLead),
            ("manager", MemberRole::Manager),
        ] {
            let actor = work
                .store
                .create_user(&work.ctx, name, &password, name)
                .unwrap();
            work.store
                .change_member_role(
                    &work.ctx,
                    &actor.id,
                    &MemberRoleChange {
                        role,
                        all_projects: false,
                        project_ids: vec![work.project.id.clone()],
                        expected_revision: 0,
                    },
                )
                .unwrap();
            let session = work.store.login(name, &password).unwrap();
            let verified = work
                .store
                .identity_gate(&session.access_token, "skip", "")
                .unwrap();
            contexts.push(
                work.store
                    .authenticate(verified["access_token"].as_str().unwrap())
                    .unwrap(),
            );
        }
        let guards = if approval {
            json!([{"kind":"human_approval"}])
        } else {
            json!([])
        };
        let flow=work.store.create_developer_workflow(&work.ctx,&serde_json::from_value(json!({"name":"Pool execution","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":guards}]}})).unwrap()).unwrap();
        let version = work
            .store
            .publish_developer_workflow(
                &work.ctx,
                &flow.id,
                &WorkflowPublish {
                    expected_revision: flow.revision,
                    reason: "Pool execution".into(),
                },
            )
            .unwrap();
        for ctx in &contexts[..2] {
            work.store
                .assign_actor_workflow(
                    &work.ctx,
                    ctx.actor_id(),
                    &DeveloperWorkflowChange {
                        expected_revision: 0,
                        workflow_version_id: version.id.clone(),
                        reason: "Published execution".into(),
                    },
                )
                .unwrap();
        }
        let manager = contexts.pop().unwrap();
        let other = contexts.pop().unwrap();
        let developer = contexts.pop().unwrap();
        Self {
            work,
            developer,
            other,
            manager,
            version,
        }
    }
    pub fn create(&mut self, assignee: Option<String>, project: Option<String>) -> WorkTask {
        self.work
            .store
            .create_pool_task(
                &self.manager,
                &PoolTaskCreate {
                    project_id: project,
                    definition: TaskCreate {
                        title: "Pending pool task".into(),
                        type_code: "task".into(),
                        assignee_id: assignee,
                        origin: "pool test".into(),
                        ..Default::default()
                    },
                },
            )
            .unwrap()
    }
}

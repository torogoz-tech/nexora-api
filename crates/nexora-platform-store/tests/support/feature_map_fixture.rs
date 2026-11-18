// Shared fixture variants are intentionally consumed by different integration tests.
#![allow(dead_code)]

#[path = "work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::{Value, json};
use work_item_fixture::WorkItemFixture;
pub struct FeatureMapFixture {
    pub work: WorkItemFixture,
    pub developer: AuthContext,
    pub pmo: AuthContext,
    pub requirement: ProjectDocument,
    pub requirement_flow: String,
    pub sql: postgres::Client,
}
impl FeatureMapFixture {
    pub fn new() -> Self {
        let mut work = WorkItemFixture::new();
        let password = zeroize::Zeroizing::new(
            nexora_platform_store::SecretGenerator::token("fixture_password").unwrap(),
        );
        let developer = work
            .store
            .create_user(&work.ctx, "developer", &password, "Developer")
            .unwrap();
        let pmo = work
            .store
            .create_user(&work.ctx, "pmo", &password, "PMO")
            .unwrap();
        for (actor, role) in [(&developer, MemberRole::Developer), (&pmo, MemberRole::Pmo)] {
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
        }
        let developer_session = work.store.login("developer", &password).unwrap();
        let verified = work
            .store
            .identity_gate(&developer_session.access_token, "skip", "")
            .unwrap();
        let developer = work
            .store
            .authenticate(verified["access_token"].as_str().unwrap())
            .unwrap();
        let pmo_session = work.store.login("pmo", &password).unwrap();
        let verified = work
            .store
            .identity_gate(&pmo_session.access_token, "skip", "")
            .unwrap();
        let pmo = work
            .store
            .authenticate(verified["access_token"].as_str().unwrap())
            .unwrap();
        let flow = work.store.create_developer_workflow(&work.ctx,&serde_json::from_value(json!({"name":"Developer execution","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap()).unwrap();
        let version = work
            .store
            .publish_developer_workflow(
                &work.ctx,
                &flow.id,
                &WorkflowPublish {
                    expected_revision: flow.revision,
                    reason: "Execution policy".into(),
                },
            )
            .unwrap();
        work.store
            .assign_actor_workflow(
                &work.ctx,
                developer.actor_id(),
                &DeveloperWorkflowChange {
                    expected_revision: 0,
                    workflow_version_id: version.id,
                    reason: "Assign execution policy".into(),
                },
            )
            .unwrap();
        let flow = work.store.create_workflow(&work.ctx,&work.project.id,&serde_json::from_value(json!({"name":"Requirements","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","terminal":true,"publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[]}]}})).unwrap()).unwrap();
        let version = work
            .store
            .publish_workflow(
                &work.ctx,
                &work.project.id,
                &flow.id,
                &WorkflowPublish {
                    expected_revision: flow.revision,
                    reason: "Requirement policy".into(),
                },
            )
            .unwrap();
        let requirement = work
            .store
            .create_project_document(
                &pmo,
                &work.project.id,
                &Self::requirement_input("REQ-001", &version.id),
            )
            .unwrap();
        let db_password = zeroize::Zeroizing::new(
            std::fs::read_to_string(work.config.secrets_directory.join("postgres_password"))
                .unwrap(),
        );
        let mut sql = postgres::Config::new()
            .host(&work.config.host)
            .port(work.config.postgres_port)
            .user(&work.config.postgres_user)
            .password(db_password.trim())
            .dbname(&work.config.postgres_database)
            .connect(postgres::NoTls)
            .unwrap();
        sql.batch_execute(&format!("SET search_path TO \"{}\"", work.namespace))
            .unwrap();
        Self {
            work,
            developer,
            pmo,
            requirement,
            requirement_flow: version.id,
            sql,
        }
    }
    pub fn requirement_content() -> Value {
        json!({"kind":"requirement","body":{"problem":"Repeated business work","objective":"Reuse governed features","scope":"Feature planning","business_actors":["Developer","PMO"],"acceptance_criteria":["Decisions precede execution"],"facts":[],"assumptions":[],"decisions":[],"open_questions":[],"source":"Acceptance test"}})
    }
    pub fn requirement_input(code: &str, _flow: &str) -> ProjectDocumentCreate {
        serde_json::from_value(json!({"code":code,"title":format!("Requirement {code}"),"content":Self::requirement_content(),"origin":"declared_business","reason":"Requirement evidence"})).unwrap()
    }
    pub fn proposal(&self, name: &str, with_task: bool) -> FeatureProposalCreate {
        FeatureProposalCreate {
            requirement_id: self.requirement.id.clone(),
            requirement_version_id: self.requirement.latest_version_id.clone(),
            expected_catalog_revision: 0,
            reason: "Reusable capability map".into(),
            links: vec![FeatureLinkProposal {
                feature_id: None,
                expected_revision: None,
                name: Some(name.into()),
                description: Some("Business capability".into()),
                impact: "Implements requirement".into(),
            }],
            tasks: if with_task {
                vec![FeatureTaskProposal {
                    feature_index: 0,
                    definition: TaskCreate {
                        title: "Implement capability".into(),
                        type_code: "task".into(),
                        assignee_id: Some(self.developer.actor_id().into()),
                        origin: "governed_feature_map".into(),
                        ..Default::default()
                    },
                }]
            } else {
                vec![]
            },
        }
    }
    pub fn decision(approved: bool) -> FeatureProposalDecision {
        FeatureProposalDecision {
            expected_revision: 1,
            decision: if approved {
                ApprovalDecision::Approved
            } else {
                ApprovalDecision::Rejected
            },
            reason: "Human reviewed requirement impact".into(),
        }
    }
    pub fn counts(&mut self) -> (i64, i64, i64, i64, i64) {
        let row=self.sql.query_one("SELECT (SELECT count(*) FROM project_features),(SELECT count(*) FROM requirement_features),(SELECT count(*) FROM tasks),(SELECT count(*) FROM task_feature_links),(SELECT count(*) FROM task_history)",&[]).unwrap();
        (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4))
    }
}

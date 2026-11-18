// Shared fixture variants are intentionally consumed by different integration tests.
#![allow(dead_code)]

#[path = "work_item_fixture.rs"]
mod work_item_fixture;
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use nexora_platform::*;
use nexora_platform_store::{GitNetworkPolicy, SecretGenerator, WebhookProof};
use serde_json::Value;
use sha2::Sha256;
use work_item_fixture::WorkItemFixture;
pub struct GitAuthorFixture {
    pub work: WorkItemFixture,
    pub sql: postgres::Client,
    pub repository: String,
    pub hook: String,
    key: zeroize::Zeroizing<Vec<u8>>,
}
impl GitAuthorFixture {
    pub fn new() -> Self {
        let mut work = WorkItemFixture::new();
        let password = zeroize::Zeroizing::new(
            std::fs::read_to_string(work.config.secrets_directory.join("postgres_password"))
                .unwrap(),
        );
        let mut sql = postgres::Config::new()
            .host(&work.config.host)
            .port(work.config.postgres_port)
            .user(&work.config.postgres_user)
            .password(password.trim())
            .dbname(&work.config.postgres_database)
            .connect(postgres::NoTls)
            .unwrap();
        sql.batch_execute(&format!(r#"SET search_path TO "{}""#, work.namespace))
            .unwrap();
        let repo = work
            .store
            .register_repository(
                &work.ctx,
                &work.project.id,
                &RepositoryRegistration {
                    provider: GitProvider::Gitlab,
                    full_name: "fixture/authors".into(),
                },
            )
            .unwrap();
        let connection = SecretGenerator::id("gcn").unwrap();
        sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')",&[&connection,&work.ctx.actor_id()]).unwrap();
        sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/authors',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2",&[&connection,&repo.id]).unwrap();
        let mut key = zeroize::Zeroizing::new(vec![0u8; 32]);
        getrandom::fill(&mut key).unwrap();
        let hook = work
            .store
            .register_gitlab_webhook(
                &work.ctx,
                &work.project.id,
                &repo.id,
                &GitlabWebhookRegistration {
                    signing_token: format!("whsec_{}", STANDARD.encode(&*key)),
                    expected_revision: 2,
                },
                &GitNetworkPolicy::default(),
            )
            .unwrap();
        Self {
            work,
            sql,
            repository: repo.id,
            hook: hook.id,
            key,
        }
    }
    pub fn member(&mut self, name: &str, role: MemberRole) -> AuthContext {
        let password = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
        let a = self
            .work
            .store
            .create_user(&self.work.ctx, name, &password, name)
            .unwrap();
        self.work
            .store
            .change_member_role(
                &self.work.ctx,
                &a.id,
                &MemberRoleChange {
                    role,
                    all_projects: false,
                    project_ids: vec![self.work.project.id.clone()],
                    expected_revision: 0,
                },
            )
            .unwrap();
        let session = self.work.store.login(name, &password).unwrap();
        let verified = self
            .work
            .store
            .identity_gate(&session.access_token, "skip", "")
            .unwrap();
        self.work
            .store
            .authenticate(verified["access_token"].as_str().unwrap())
            .unwrap()
    }
    pub fn proof(&self, id: &str, payload: Value) -> (WebhookProof, Vec<u8>) {
        let body = serde_json::to_vec(&payload).unwrap();
        let timestamp = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.key).unwrap();
        mac.update(format!("{id}.{timestamp}.").as_bytes());
        mac.update(&body);
        (
            WebhookProof {
                delivery_id: id.into(),
                event: "Push Hook".into(),
                signature: format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes())),
                timestamp: Some(timestamp),
            },
            body,
        )
    }
    pub fn send(&mut self, id: &str, payload: Value) -> WebhookReceipt {
        let body = serde_json::to_vec(&payload).unwrap();
        let timestamp = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.key).unwrap();
        mac.update(format!("{id}.{timestamp}.").as_bytes());
        mac.update(&body);
        let proof = WebhookProof {
            delivery_id: id.into(),
            event: "Push Hook".into(),
            signature: format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes())),
            timestamp: Some(timestamp),
        };
        self.work
            .store
            .receive_gitlab_webhook(&self.hook, &proof, &body, &GitNetworkPolicy::default())
            .unwrap()
    }
}

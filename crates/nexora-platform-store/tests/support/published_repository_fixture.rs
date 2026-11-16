#[path = "work_item_fixture.rs"]
pub(crate) mod work_item_fixture;
use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use nexora_runtime::DatabaseRuntime;
use std::path::{Path, PathBuf};
use work_item_fixture::WorkItemFixture;
use zeroize::Zeroizing;

/// Provider consent is a local SQL fixture; snapshots follow the real stage/publication path.
pub struct PublishedRepositoryFixture {
    pub work: WorkItemFixture,
    pub runtime: DatabaseRuntime,
    pub sql: postgres::Client,
    pub root: PathBuf,
    pub binding: RepositoryBinding,
    pub connection: String,
}
impl PublishedRepositoryFixture {
    pub fn new() -> Self {
        let mut work = WorkItemFixture::new();
        work.task("Keep work fixture policy available");
        let password = Zeroizing::new(
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
        sql.batch_execute(&format!("SET search_path TO \"{}\"", work.namespace))
            .unwrap();
        let connection = SecretGenerator::id("conn").unwrap();
        sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')",&[&connection,&work.ctx.actor_id()]).unwrap();
        sql.execute("INSERT INTO git_connection_projects(connection_id,project_id,granted_by) VALUES($1,$2,$3)",&[&connection,&work.project.id,&work.ctx.actor_id()]).unwrap();
        let repository = work
            .store
            .register_repository(
                &work.ctx,
                &work.project.id,
                &RepositoryRegistration {
                    provider: GitProvider::Gitlab,
                    full_name: "fixture/registered-knowledge".into(),
                },
            )
            .unwrap();
        // This fixture exercises the legacy single-indexer publication path explicitly.
        work.store
            .change_repository_analysis_profile(
                &work.ctx,
                &work.project.id,
                &repository.id,
                &nexora_platform::RepositoryAnalysisProfileChange {
                    expected_revision: 0,
                    profile_version_id: None,
                    reason: "Legacy publication fixture".into(),
                    mode: None,
                    profile_version_ids: None,
                },
            )
            .unwrap();
        sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name=requested_full_name,connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2",&[&connection,&repository.id]).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(SecretGenerator::id("nexora-published-source").unwrap());
        std::fs::create_dir(&root).unwrap();
        let mut runtime = DatabaseRuntime::connect(&work.config).unwrap();
        let binding = Self::publish(&mut work, &mut runtime, &repository.id, &root, 1);
        Self {
            work,
            runtime,
            sql,
            root,
            binding,
            connection,
        }
    }
    fn publish(
        work: &mut WorkItemFixture,
        runtime: &mut DatabaseRuntime,
        repository: &str,
        root: &Path,
        seed: u8,
    ) -> RepositoryBinding {
        work.store
            .enqueue_repository_job(
                &work.ctx,
                &work.project.id,
                repository,
                RepositoryJobKind::Index,
                &RepositoryJobRequest {
                    expected_revision: 2,
                },
            )
            .unwrap();
        let lease = work
            .store
            .claim_repository_job("knowledge-fixture-publisher", 300)
            .unwrap()
            .unwrap();
        assert_eq!(lease.job.repository_id, repository);
        let commit = char::from(b'a' + seed % 26).to_string().repeat(40);
        let scope = work.store.pin_repository_job(&lease, &commit).unwrap();
        std::fs::write(
            root.join("source.js"),
            format!("function source_{seed}() {{ return {seed}; }}\n"),
        )
        .unwrap();
        let indexer = Indexer::new(
            LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap(),
        );
        let staged = runtime
            .stage_repository(
                &indexer,
                root,
                TenantId(scope.tenant_id.clone()),
                RepositoryId(scope.repository_id.clone()),
            )
            .unwrap();
        let header = runtime
            .query_header(
                &TenantId(scope.tenant_id),
                &RepositoryId(scope.repository_id),
                Some(&staged.snapshot.id),
            )
            .unwrap();
        work.store.complete_repository_job(&lease, &header).unwrap()
    }
    pub fn publish_revision(&mut self, seed: u8) -> RepositoryBinding {
        self.binding = Self::publish(
            &mut self.work,
            &mut self.runtime,
            &self.binding.repository_id,
            &self.root,
            seed,
        );
        self.binding.clone()
    }
    pub fn contributor(&mut self, name: &str, write: bool) -> AuthContext {
        let actor = self
            .work
            .store
            .create_actor(
                &self.work.ctx,
                ActorKind::Agent,
                name,
                "Source-backed external knowledge client",
            )
            .unwrap();
        let mut capabilities = vec![Capability::RepositoriesRead, Capability::KnowledgeRead];
        if write {
            capabilities.push(Capability::KnowledgeWrite);
        }
        for capability in &capabilities {
            self.work
                .store
                .grant(
                    &self.work.ctx,
                    &actor.id,
                    *capability,
                    Some(&self.work.project.id),
                )
                .unwrap();
        }
        let key = self
            .work
            .store
            .issue_key(
                &self.work.ctx,
                &actor.id,
                "Knowledge client",
                KeyScope {
                    capabilities,
                    project_ids: vec![self.work.project.id.clone()],
                    all_projects: false,
                    installation: false,
                },
                3600,
            )
            .unwrap();
        self.work.store.authenticate(&key.key).unwrap()
    }
}

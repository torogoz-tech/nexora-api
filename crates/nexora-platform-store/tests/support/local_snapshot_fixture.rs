use crate::work_item_fixture::WorkItemFixture;
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_core::TenantId;
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{LocalSnapshotAdoption, LocalSnapshotPublication};
use nexora_platform_store::SecretGenerator;
use nexora_runtime::{DatabaseRuntime, KnowledgeCommand, QuerySnapshot};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// Public source analyzed with the original root-derived runtime identity.
pub struct LocalSnapshotFixture {
    pub work: WorkItemFixture,
    pub runtime: DatabaseRuntime,
    pub sql: postgres::Client,
    pub root: PathBuf,
    pub header: QuerySnapshot,
}
impl LocalSnapshotFixture {
    #[allow(dead_code)] // Shared by the subsequent source-read and lifecycle targets.
    pub fn adopt(&mut self) -> LocalSnapshotPublication {
        let input: LocalSnapshotAdoption = serde_json::from_value(json!({
            "name":"Local fixture","root":self.root,"runtime_tenant_id":self.header.tenant_id.0,
            "snapshot_id":self.header.id.0,"expected_payload_hash":self.header.payload_hash,
            "reason":"Attest public fixture for authorized source reads"
        }))
        .unwrap();
        let project = self.work.project.id.clone();
        let operator = self.work.ctx.clone();
        let canonical = json!({"project_id":project,"root_digest":self.header.repository_id.0,
            "runtime_tenant_id":input.runtime_tenant_id,"snapshot_id":input.snapshot_id,
            "expected_payload_hash":input.expected_payload_hash,"name":input.name,"reason":input.reason});
        self.work
            .store
            .request(
                &operator,
                "local_snapshot.adopt",
                "adopt-fixture",
                &canonical,
                |store| store.adopt_local_snapshot(&operator, &project, &input),
            )
            .unwrap()
    }
    #[allow(dead_code)] // Shared fixture; only the integrity target mutates its own projection.
    pub fn projection(&self, statement: &str) -> Value {
        let auth = Zeroizing::new(
            std::fs::read_to_string(self.work.config.secrets_directory.join("neo4j_auth")).unwrap(),
        );
        let (user, password) = auth.trim().split_once('/').unwrap();
        let header = Zeroizing::new(format!(
            "Basic {}",
            STANDARD.encode(format!("{user}:{password}"))
        ));
        let scope = nexora_runtime::Neo4jProjection::scope(
            &self.header.tenant_id,
            &self.header.repository_id,
            &self.header.id,
        )
        .unwrap();
        let mut response = ureq::post(format!(
            "http://{}:{}/db/neo4j/tx/commit",
            self.work.config.host, self.work.config.neo4j_http_port
        ))
        .header("Authorization", header.as_str())
        .send_json(json!({"statements":[{"statement":statement,"parameters":{"scope":scope}}]}))
        .unwrap();
        let result: Value = response.body_mut().read_json().unwrap();
        assert_eq!(result["errors"], json!([]));
        result["results"][0]["data"][0]["row"][0].clone()
    }
    pub fn new() -> Self {
        let mut work = WorkItemFixture::new();
        work.task("Keep the shared work fixture policy exercised");
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
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(SecretGenerator::id("nexora-local-snapshot").unwrap());
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("source.js"),
            "function retained_identity() { return 42; }\n",
        )
        .unwrap();
        let indexer = Indexer::new(
            LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap(),
        );
        let mut runtime = DatabaseRuntime::connect(&work.config).unwrap();
        let tenant = TenantId(SecretGenerator::id("legacy-fixture").unwrap());
        let indexed = runtime.index(&indexer, &root, tenant).unwrap();
        let snapshot = indexed.snapshot;
        let header = runtime
            .query_header(
                &snapshot.tenant_id,
                &snapshot.repository_id,
                Some(&snapshot.id),
            )
            .unwrap();
        let contribution: KnowledgeCommand = serde_json::from_value(json!({
            "operation":"contribute","request_id":"before-adoption",
            "draft":{"kind":"decision","statement":"Preserve the original runtime identity and contribution.",
                "evidence":[{"path":"source.js","start_line":1,"line_count":1}]}
        })).unwrap();
        runtime
            .knowledge(
                &header.tenant_id,
                &header.repository_id,
                Some(&header.id),
                "legacy-author",
                &contribution,
            )
            .unwrap();
        Self {
            work,
            runtime,
            sql,
            root,
            header,
        }
    }
}

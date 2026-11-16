#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_language::AnalysisProfilePackage;
use nexora_platform::*;
use serde_json::{Value, json};
use std::path::Path;
fn package(version: &str) -> AnalysisProfilePackage {
    AnalysisProfilePackage::from_catalog(
        Path::new("../../languages/lua"),
        "lua".into(),
        "lua-standard".into(),
        version.into(),
        "Lua standard".into(),
    )
    .unwrap()
}
fn change(revision: i64, mode: &str, ids: Vec<String>) -> RepositoryAnalysisProfileChange {
    RepositoryAnalysisProfileChange {
        expected_revision: revision,
        profile_version_id: None,
        reason: "Choose analysis policy".into(),
        mode: Some(mode.into()),
        profile_version_ids: Some(ids),
    }
}
#[test]
#[ignore = "requires local PostgreSQL; candidate freezing, legacy worker fence and manual validation"]
fn jobs_pin_candidate_sets_and_enforce_worker_upgrade_and_manual_conflicts() {
    let mut f = GitAuthorFixture::new();
    let initial = f
        .work
        .store
        .repository_analysis_profile(&f.work.ctx, &f.work.project.id, &f.repository)
        .unwrap();
    assert_eq!(initial["mode"], "automatic");
    assert_eq!(initial["revision"], 0);
    let first = f
        .work
        .store
        .import_analysis_profile(&f.work.ctx, &package("1.0.0"))
        .unwrap();
    let request = RepositoryJobRequest {
        expected_revision: 2,
    };
    let job = f
        .work
        .store
        .enqueue_repository_job(
            &f.work.ctx,
            &f.work.project.id,
            &f.repository,
            RepositoryJobKind::Index,
            &request,
        )
        .unwrap();
    let frozen: Value = f
        .sql
        .query_one(
            "SELECT analysis_profile_plan FROM repository_jobs WHERE id=$1",
            &[&job.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(frozen["profile_version_ids"], json!([first["id"]]));
    assert!(
        f.sql
            .execute(
                "UPDATE repository_jobs SET state='running',attempt=attempt+1 WHERE id=$1",
                &[&job.id]
            )
            .is_err()
    );
    let second = f
        .work
        .store
        .import_analysis_profile(&f.work.ctx, &package("2.0.0"))
        .unwrap();
    assert!(matches!(
        f.work.store.change_repository_analysis_profile(
            &f.work.ctx,
            &f.work.project.id,
            &f.repository,
            &change(
                0,
                "manual",
                vec![
                    first["id"].as_str().unwrap().into(),
                    second["id"].as_str().unwrap().into()
                ]
            )
        ),
        Err(PlatformError::Validation(_))
    ));
    let manual = f
        .work
        .store
        .change_repository_analysis_profile(
            &f.work.ctx,
            &f.work.project.id,
            &f.repository,
            &change(0, "manual", vec![second["id"].as_str().unwrap().into()]),
        )
        .unwrap();
    assert_eq!(manual["mode"], "manual");
    let lease = f
        .work
        .store
        .claim_repository_job("multi_profiles", 300)
        .unwrap()
        .unwrap();
    let loaded = f
        .work
        .store
        .repository_job_analysis_profiles(&lease)
        .unwrap()
        .unwrap();
    assert_eq!(loaded["mode"], "automatic");
    assert_eq!(loaded["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(loaded["candidates"][0]["id"], first["id"]);
    assert!(
        loaded["candidates"][0]["created_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert!(
        f.sql
            .execute(
                "UPDATE repository_jobs SET analysis_profile_plan='{}' WHERE id=$1",
                &[&job.id]
            )
            .is_err()
    );
    // Releasing an expired lease creates a new attempt but never resamples imports or configuration.
    f.sql.execute("UPDATE repository_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1",&[&job.id]).unwrap();
    let retry = f
        .work
        .store
        .claim_repository_job("multi_profiles", 300)
        .unwrap()
        .unwrap();
    assert_eq!(retry.job.id, job.id);
    assert_eq!(
        f.work
            .store
            .repository_job_analysis_profiles(&retry)
            .unwrap()
            .unwrap(),
        loaded
    );
    let reader = f.member("profile_reader", MemberRole::Developer);
    assert!(matches!(
        f.work.store.change_repository_analysis_profile(
            &reader,
            &f.work.project.id,
            &f.repository,
            &change(1, "automatic", vec![])
        ),
        Err(PlatformError::Forbidden)
    ));
    let visible = f
        .work
        .store
        .repository_analysis_profile(&reader, &f.work.project.id, &f.repository)
        .unwrap();
    assert_eq!(visible["can_manage"], false);
    assert!(visible["profiles"][0].get("package").is_none());
    // Scalar requests remain supported and keep their old serialized request fingerprint.
    let scalar:RepositoryAnalysisProfileChange=serde_json::from_value(json!({"expected_revision":1,"profile_version_id":null,"reason":"Use legacy worker fallback"})).unwrap();
    assert!(serde_json::to_value(&scalar).unwrap().get("mode").is_none());
    let legacy = f
        .work
        .store
        .change_repository_analysis_profile(&f.work.ctx, &f.work.project.id, &f.repository, &scalar)
        .unwrap();
    assert_eq!(legacy["mode"], "legacy");
}
#[test]
#[ignore = "requires local databases; atomic profile provenance and binding"]
fn selection_report_is_validated_and_committed_with_binding() {
    use nexora_core::{RepositoryId, TenantId};
    use nexora_indexer::AnalysisProfileCompiler;
    use nexora_runtime::DatabaseRuntime;
    let mut f = GitAuthorFixture::new();
    let package = package("1.0.0");
    let profile = f
        .work
        .store
        .import_analysis_profile(&f.work.ctx, &package)
        .unwrap();
    let job = f
        .work
        .store
        .enqueue_repository_job(
            &f.work.ctx,
            &f.work.project.id,
            &f.repository,
            RepositoryJobKind::Index,
            &RepositoryJobRequest {
                expected_revision: 2,
            },
        )
        .unwrap();
    let lease = f
        .work
        .store
        .claim_repository_job("profile_publication", 300)
        .unwrap()
        .unwrap();
    f.work
        .store
        .pin_repository_job(&lease, &"a".repeat(40))
        .unwrap();
    let tenant: String = f
        .sql
        .query_one("SELECT id FROM installation WHERE singleton", &[])
        .unwrap()
        .get(0);
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(nexora_platform_store::SecretGenerator::id("profile-publication").unwrap());
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("source.lua"), "local result = 1\n").unwrap();
    let mut runtime = DatabaseRuntime::connect(&f.work.config).unwrap();
    let staged = runtime
        .stage_repository(
            &AnalysisProfileCompiler::compile(&package).unwrap(),
            &root,
            TenantId(tenant.clone()),
            RepositoryId(f.repository.clone()),
        )
        .unwrap();
    let header = runtime
        .query_header(
            &TenantId(tenant),
            &RepositoryId(f.repository.clone()),
            Some(&staged.snapshot.id),
        )
        .unwrap();
    let mut summary = profile.clone();
    summary.as_object_mut().unwrap().remove("created_at");
    summary["files"] = json!(["source.lua"]);
    let report = json!({"policy":"automatic-latest/v1","status":"partial","profiles":[summary],"uncovered_files":[],"excluded_files":[],"excluded_directories":[],"skipped_files":[{"path":"Legacy.lua","profile_id":profile["id"],"reason":"invalid UTF-8 at byte 9","checksum":"a".repeat(64)}],"decoded_files":[{"path":"source.lua","encoding":"UTF-16LE","checksum":"b".repeat(64)}]});
    for edit in [
        json!({"status":"complete"}),
        json!({"decoded_files":[{"path":"Missing.lua","encoding":"UTF-16LE","checksum":"b".repeat(64)}]}),
        json!({"skipped_files":[{"path":"source.lua","profile_id":profile["id"],"reason":"invalid UTF-8","checksum":"a".repeat(64)}]}),
        json!({"skipped_files":[{"path":"Legacy.lua","profile_id":"not-pinned","reason":"invalid UTF-8","checksum":"a".repeat(64)}]}),
    ] {
        let mut invalid = report.clone();
        for (key, value) in edit.as_object().unwrap() {
            invalid[key] = value.clone();
        }
        assert!(
            f.work
                .store
                .complete_repository_job_with_profile_selection(&lease, &header, &invalid)
                .is_err()
        );
    }
    assert!(matches!(
        f.work.store.complete_repository_job(&lease, &header),
        Err(PlatformError::Validation(_))
    ));
    let mut invalid = report.clone();
    invalid["profiles"][0]["checksum"] = json!("wrong");
    assert!(
        f.work
            .store
            .complete_repository_job_with_profile_selection(&lease, &header, &invalid)
            .is_err()
    );
    assert_eq!(
        f.sql
            .query_one(
                "SELECT count(*) FROM repository_bindings WHERE repository_id=$1",
                &[&f.repository]
            )
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let binding = f
        .work
        .store
        .complete_repository_job_with_profile_selection(&lease, &header, &report)
        .unwrap();
    assert_eq!(binding.job_id, job.id);
    assert_eq!(
        f.work
            .store
            .complete_repository_job_with_profile_selection(&lease, &header, &report)
            .unwrap()
            .snapshot_id,
        binding.snapshot_id
    );
    assert_eq!(
        f.work
            .store
            .repository_analysis_profile(&f.work.ctx, &f.work.project.id, &f.repository)
            .unwrap()["last_selection"],
        report
    );
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Repository metadata only",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let metadata = f.work.store.authenticate(&key.key).unwrap();
    assert!(
        f.work
            .store
            .repository_analysis_profile(&metadata, &f.work.project.id, &f.repository)
            .unwrap()["last_selection"]
            .is_null()
    );
    f.sql
        .execute(
            "UPDATE project_repositories SET authorization_status='revoked' WHERE id=$1",
            &[&f.repository],
        )
        .unwrap();
    assert!(
        f.work
            .store
            .repository_analysis_profile(&f.work.ctx, &f.work.project.id, &f.repository)
            .unwrap()["last_selection"]
            .is_null()
    );
    assert!(
        f.sql
            .execute(
                "UPDATE repository_jobs SET analysis_profile_selection='{}' WHERE id=$1",
                &[&job.id]
            )
            .is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
#[ignore = "requires local PostgreSQL; upgrade preserves preexisting manual pins and fallback"]
fn migration_preserves_existing_configuration_and_historical_jobs() {
    use nexora_runtime::DatabaseConnections;
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let secret = zeroize::Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(secret.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    let namespace = nexora_platform_store::SecretGenerator::id("multi_migration").unwrap();
    let mut tx = sql.transaction().unwrap();
    tx.batch_execute(&format!(
        "CREATE SCHEMA {namespace}; SET LOCAL search_path TO {namespace}"
    ))
    .unwrap();
    let mut migrations = std::fs::read_dir("migrations")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect::<Vec<_>>();
    migrations.sort();
    for path in migrations.iter().filter(|path| {
        path.file_name().unwrap().to_string_lossy().as_ref() < "0057_multi_analysis_profiles.sql"
    }) {
        tx.batch_execute(&std::fs::read_to_string(path).unwrap())
            .unwrap();
    }
    tx.batch_execute("INSERT INTO actors(id,kind,name,purpose) VALUES('actor_upgrade','human','Upgrade','fixture'); INSERT INTO projects(id,name,normalized_name) VALUES('prj_upgrade','Upgrade','upgrade'); INSERT INTO project_repositories(id,project_id,provider,host,requested_full_name) VALUES('repo_manual','prj_upgrade','github','github.com','fixture/manual'),('repo_legacy','prj_upgrade','github','github.com','fixture/legacy'); INSERT INTO analysis_profile_versions(id,key,version,name,language_id,checksum,package,created_by) VALUES('apv_old','old','1.0.0','Old','lua','checksum','{}','actor_upgrade'); INSERT INTO repository_analysis_profiles(repository_id,profile_version_id,revision,changed_by) VALUES('repo_manual','apv_old',7,'actor_upgrade')").unwrap();
    tx.batch_execute(include_str!(
        "../migrations/0057_multi_analysis_profiles.sql"
    ))
    .unwrap();
    let row=tx.query_one("SELECT mode,profile_version_ids,revision FROM repository_analysis_profiles WHERE repository_id='repo_manual'",&[]).unwrap();
    assert_eq!(row.get::<_, String>(0), "manual");
    assert_eq!(row.get::<_, Value>(1), json!(["apv_old"]));
    assert_eq!(row.get::<_, i64>(2), 7);
    assert_eq!(
        tx.query_one(
            "SELECT mode FROM repository_analysis_profiles WHERE repository_id='repo_legacy'",
            &[]
        )
        .unwrap()
        .get::<_, String>(0),
        "legacy"
    );
    tx.rollback().unwrap();
}
#[test]
#[ignore = "requires local PostgreSQL; concurrent import transaction and enqueue have one candidate snapshot"]
fn enqueue_waits_for_import_commit_and_then_keeps_exact_candidates() {
    use std::time::{Duration, Instant};
    let mut f = GitAuthorFixture::new();
    let first = f
        .work
        .store
        .import_analysis_profile(&f.work.ctx, &package("1.0.0"))
        .unwrap();
    let second = package("2.0.0");
    let second_id = nexora_platform_store::SecretGenerator::id("apv").unwrap();
    let second_package = serde_json::to_value(second.canonical()).unwrap();
    let checksum = second.checksum().unwrap();
    let mut importing = f.sql.transaction().unwrap();
    let key: i64 = importing
        .query_one(
            "SELECT hashtextextended(current_schema()||':analysis-profile-import',0)",
            &[],
        )
        .unwrap()
        .get(0);
    importing
        .query_one("SELECT pg_advisory_xact_lock($1)", &[&key])
        .unwrap();
    importing.execute("INSERT INTO analysis_profile_versions(id,key,version,name,language_id,checksum,package,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&second_id,&second.key,&second.version,&second.name,&second.language_id,&checksum,&second_package,&f.work.ctx.actor_id()]).unwrap();
    let config = f.work.config.clone();
    let namespace = f.work.namespace.clone();
    let ctx = f.work.ctx.clone();
    let project = f.work.project.id.clone();
    let repository = f.repository.clone();
    let enqueue = std::thread::spawn(move || {
        let mut store = nexora_platform_store::PlatformStore::connect(&config, &namespace).unwrap();
        store
            .enqueue_repository_job(
                &ctx,
                &project,
                &repository,
                RepositoryJobKind::Index,
                &RepositoryJobRequest {
                    expected_revision: 2,
                },
            )
            .unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let high = ((key as u64) >> 32) as i64;
    let low = ((key as u64) & 0xffff_ffff) as i64;
    loop {
        let waiting:bool=importing.query_one("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND classid::bigint=$1 AND objid::bigint=$2)",&[&high,&low]).unwrap().get(0);
        if waiting {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "enqueue must wait for the concurrent catalog import"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    importing.commit().unwrap();
    let job = enqueue.join().unwrap();
    f.work
        .store
        .import_analysis_profile(&f.work.ctx, &package("3.0.0"))
        .unwrap();
    let lease = f
        .work
        .store
        .claim_repository_job("import_race", 300)
        .unwrap()
        .unwrap();
    assert_eq!(job.id, lease.job.id);
    let loaded = f
        .work
        .store
        .repository_job_analysis_profiles(&lease)
        .unwrap()
        .unwrap();
    let mut actual = loaded["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = vec![first["id"].as_str().unwrap().to_string(), second_id];
    expected.sort();
    assert_eq!(actual, expected);
}

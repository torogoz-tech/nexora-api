#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; personal MCP upgrade preserves read-only and service-key boundaries"]
fn personal_keys_upgrade_without_changing_service_keys() {
    let mut f = WorkItemFixture::new();
    let input = PersonalMcpKeyCreate {
        name: "Migration fixture".into(),
        project_ids: vec![],
        read_only: false,
        include_global: false,
        expires_in_seconds: 86400,
    };
    let writable = f.store.issue_personal_mcp_key(&f.ctx, &input).unwrap();
    let readonly = f
        .store
        .issue_personal_mcp_key(
            &f.ctx,
            &PersonalMcpKeyCreate {
                read_only: true,
                ..input.clone()
            },
        )
        .unwrap();
    let service = f.store.issue_personal_mcp_key(&f.ctx, &input).unwrap();
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
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    let old_write = json!({"capabilities":["tasks:read","tasks:write"],"project_ids":[f.project.id],"all_projects":false,"installation":false});
    let old_read = json!({"capabilities":["tasks:read"],"project_ids":[f.project.id],"all_projects":false,"installation":false});
    sql.execute(
        "UPDATE api_keys SET scope=$2 WHERE id=$1",
        &[&writable.metadata.id, &old_write],
    )
    .unwrap();
    sql.execute(
        "UPDATE api_keys SET scope=$2 WHERE id=$1",
        &[&readonly.metadata.id, &old_read],
    )
    .unwrap();
    sql.execute(
        "UPDATE api_keys SET scope=$2,personal_mcp=false WHERE id=$1",
        &[&service.metadata.id, &old_write],
    )
    .unwrap();
    sql.batch_execute(include_str!(
        "../migrations/0094_personal_mcp_live_access.sql"
    ))
    .unwrap();
    let write = f.store.authenticate(&writable.key).unwrap();
    let read = f.store.authenticate(&readonly.key).unwrap();
    assert!(write.scope().unwrap().all_projects);
    assert!(
        write
            .require(Capability::ProjectsRead, Some(&f.project.id))
            .is_ok()
    );
    assert!(
        write
            .require(Capability::TasksWrite, Some(&f.project.id))
            .is_ok()
    );
    assert!(
        read.require(Capability::TasksWrite, Some(&f.project.id))
            .is_err()
    );
    assert!(
        read.require(Capability::ProjectsRead, Some(&f.project.id))
            .is_ok()
    );
    assert!(write.require(Capability::InstallationManage, None).is_err());
    assert!(
        write
            .require(Capability::ApprovalsGrant, Some(&f.project.id))
            .is_err()
    );
    let unchanged: serde_json::Value = sql
        .query_one(
            "SELECT scope FROM api_keys WHERE id=$1",
            &[&service.metadata.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(unchanged, old_write);
    // The migration envelope stays in sync with issuance and is idempotent.
    for (current, issued) in [
        (write.scope().unwrap(), &writable.metadata.scope),
        (read.scope().unwrap(), &readonly.metadata.scope),
    ] {
        assert_eq!(
            current
                .capabilities
                .iter()
                .collect::<std::collections::BTreeSet<_>>(),
            issued
                .capabilities
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
        );
        assert_eq!(current.project_ids, issued.project_ids);
        assert_eq!(current.all_projects, issued.all_projects);
        assert_eq!(current.installation, issued.installation);
    }
    sql.batch_execute(include_str!(
        "../migrations/0094_personal_mcp_live_access.sql"
    ))
    .unwrap();
    assert!(
        f.store
            .authenticate(&readonly.key)
            .unwrap()
            .require(Capability::TasksWrite, Some(&f.project.id))
            .is_err()
    );
}

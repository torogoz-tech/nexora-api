use nexora_platform_store::PlatformStore;
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
#[test]
#[ignore = "requires PostgreSQL; preserves published task policy content on upgrade and rolls back ambiguous developer membership"]
fn developer_workflow_upgrade_preserves_versions_or_rolls_back_without_guessing() {
    let checksum_of = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut client = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    for ambiguous in [false, true] {
        let namespace = format!(
            "nexora_platform_devupgrade_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        client
            .batch_execute(&format!(
                "CREATE SCHEMA \"{namespace}\";SET search_path TO \"{namespace}\";"
            ))
            .unwrap();
        let mut migrations = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        migrations.sort();
        for path in migrations {
            let filename = path.file_name().unwrap().to_str().unwrap();
            let version: i32 = filename.split('_').next().unwrap().parse().unwrap();
            if version > 29 {
                continue;
            }
            let sql = std::fs::read_to_string(path).unwrap();
            client.batch_execute(&sql).unwrap();
            client
                .execute(
                    "INSERT INTO platform_schema_versions(version,checksum) VALUES($1,$2)",
                    &[&version, &checksum_of(sql.as_bytes())],
                )
                .unwrap();
        }
        client.batch_execute("INSERT INTO actors(id,kind,name,purpose) VALUES('developer','human','Developer','Migration fixture');INSERT INTO projects(id,name,normalized_name) VALUES('project','Project','project');INSERT INTO task_types(project_id,code,name) VALUES('project','task','Task');").unwrap();
        let graph = json!({"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]});
        let kinds = json!(["task"]);
        let checksum = checksum_of(
            &serde_json::to_vec(
                &json!({"name":"Flow","purpose":"","resource_kinds":kinds,"graph":graph}),
            )
            .unwrap(),
        );
        for index in 0..if ambiguous { 2 } else { 1 } {
            let workflow = format!("flow_{index}");
            let version = format!("version_{index}");
            let task = format!("task_{index}");
            client.execute("INSERT INTO workflows(id,project_id,name,purpose,resource_kinds,draft,created_by) VALUES($1,'project','Flow','',$2,$3,'developer')",&[&workflow,&kinds,&graph]).unwrap();
            client.execute("INSERT INTO workflow_versions(id,workflow_id,project_id,number,name,purpose,resource_kinds,graph,checksum,reason,published_by) VALUES($1,$2,'project',1,'Flow','',$3,$4,$5,'Published','developer')",&[&version,&workflow,&kinds,&graph,&checksum]).unwrap();
            client
                .execute(
                    "UPDATE workflows SET latest_version_id=$1,latest_version_number=1 WHERE id=$2",
                    &[&version, &workflow],
                )
                .unwrap();
            client.execute("INSERT INTO tasks(id,project_id,title,context,type_code,priority,assignee_id,workflow_id,workflow_version_id,state_id,origin,created_by) VALUES($1,'project','Work','','task','medium','developer',$2,$3,'ready','fixture','developer')",&[&task,&workflow,&version]).unwrap();
        }
        let mut store = PlatformStore::connect(&config, &namespace).unwrap();
        let result = store.initialize();
        if ambiguous {
            assert!(result.is_err());
            assert_eq!(
                client
                    .query_one("SELECT max(version) FROM platform_schema_versions", &[])
                    .unwrap()
                    .get::<_, i32>(0),
                29
            );
            assert_eq!(
                client
                    .query_one("SELECT project_id FROM workflows WHERE id='flow_0'", &[])
                    .unwrap()
                    .get::<_, String>(0),
                "project"
            );
        } else {
            result.unwrap();
            store.initialize().unwrap();
            assert_eq!(
                client
                    .query_one("SELECT project_id FROM workflows WHERE id='flow_0'", &[])
                    .unwrap()
                    .get::<_, Option<String>>(0),
                None
            );
            assert_eq!(
                client
                    .query_one(
                        "SELECT checksum FROM workflow_versions WHERE id='version_0'",
                        &[]
                    )
                    .unwrap()
                    .get::<_, String>(0),
                checksum
            );
            assert_eq!(
                client
                    .query_one(
                        "SELECT workflow_id FROM developer_workflows WHERE actor_id='developer'",
                        &[]
                    )
                    .unwrap()
                    .get::<_, String>(0),
                "flow_0"
            );
        }
        drop(store);
        client
            .batch_execute(&format!("DROP SCHEMA \"{namespace}\" CASCADE"))
            .unwrap();
    }
}

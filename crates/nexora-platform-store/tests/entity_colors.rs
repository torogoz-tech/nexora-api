#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; color roundtrips and immutable workflow versions"]
fn entity_colors_persist_clear_and_preserve_revision_and_workflow_contracts() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Color is inherited display metadata");
    assert!(serde_json::to_value(task).unwrap().get("color").is_none());
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
    for invalid in ["red", "#123", "#aabbcc"] {
        assert!(
            sql.execute(
                "UPDATE projects SET color=$2 WHERE id=$1",
                &[&f.project.id, &invalid]
            )
            .is_err()
        );
    }

    let legacy_definition = json!({"code":"legacy","name":"Legacy","description":null});
    let definition: TaskTypeCreate = serde_json::from_value(legacy_definition.clone()).unwrap();
    let old_input = json!({"project_id":f.project.id,"definition":legacy_definition});
    let original = f
        .store
        .request(
            &f.ctx,
            "task_type.create",
            "pre-color-retry",
            &old_input,
            |store| store.create_task_type(&f.ctx, &f.project.id, &definition),
        )
        .unwrap();
    let input = json!({"project_id":f.project.id,"definition":definition});
    let replay = f
        .store
        .request(
            &f.ctx,
            "task_type.create",
            "pre-color-retry",
            &input,
            |store| store.create_task_type(&f.ctx, &f.project.id, &definition),
        )
        .unwrap();
    assert_eq!(replay.code, original.code);
    assert_eq!(replay.revision, original.revision);
    let kind = f
        .store
        .create_task_type(
            &f.ctx,
            &f.project.id,
            &serde_json::from_value(json!({"code":"color_bug","name":"Bug","color":"#aa0011"}))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(kind.color.as_ref().map(HexColor::as_str), Some("#AA0011"));
    let color_only =
        json!({"expected_revision":kind.revision,"color":"#112233","reason":"Adjust display"});
    let changed = f
        .store
        .change_task_type(
            &f.ctx,
            &f.project.id,
            "color_bug",
            &serde_json::from_value(color_only.clone()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        changed.color.as_ref().map(HexColor::as_str),
        Some("#112233")
    );
    assert!(matches!(
        f.store.change_task_type(
            &f.ctx,
            &f.project.id,
            "color_bug",
            &serde_json::from_value(color_only).unwrap()
        ),
        Err(PlatformError::Conflict(_))
    ));
    let preserved = f
        .store
        .change_task_type(
            &f.ctx,
            &f.project.id,
            "color_bug",
            &serde_json::from_value(
                json!({"expected_revision":changed.revision,"name":"Defect","reason":"Rename"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(preserved.color, changed.color);
    let cleared = f
        .store
        .change_task_type(
            &f.ctx,
            &f.project.id,
            "color_bug",
            &serde_json::from_value(
                json!({"expected_revision":preserved.revision,"color":null,"reason":"Use default"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(cleared.color.is_none());

    let backlog = f.store.create_backlog(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Colored","workflow_version_id":f.backlog.workflow_version_id,"color":"#bbccdd"})).unwrap()).unwrap();
    assert_eq!(
        backlog.color.as_ref().map(HexColor::as_str),
        Some("#BBCCDD")
    );
    assert_eq!(
        f.store
            .get_backlog(&f.ctx, &f.project.id, &backlog.id)
            .unwrap()
            .color,
        backlog.color
    );
    let preserved = f
        .store
        .change_backlog(
            &f.ctx,
            &f.project.id,
            &backlog.id,
            &serde_json::from_value(
                json!({"expected_revision":backlog.revision,"name":"Renamed","reason":"Rename"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(preserved.color, backlog.color);
    let cleared = f
        .store
        .change_backlog(
            &f.ctx,
            &f.project.id,
            &backlog.id,
            &serde_json::from_value(
                json!({"expected_revision":preserved.revision,"color":null,"reason":"Use default"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(cleared.color.is_none());

    let color: HexColor = serde_json::from_value(json!("#123abc")).unwrap();
    let project = f
        .store
        .create_project_with_color(&f.ctx, "Colored project", None, Some(&color))
        .unwrap();
    assert_eq!(
        project.color.as_ref().map(HexColor::as_str),
        Some("#123ABC")
    );
    let preserved = f
        .store
        .change_project(
            &f.ctx,
            &project.id,
            &serde_json::from_value(
                json!({"expected_revision":project.revision,"name":project.name,"archived":false}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(preserved.color, project.color);
    let changed = f.store.change_project(&f.ctx,&project.id,&serde_json::from_value(json!({"expected_revision":preserved.revision,"name":project.name,"archived":false,"color":null})).unwrap()).unwrap();
    assert!(changed.color.is_none());
    assert!(
        f.store
            .get_project(&f.ctx, &project.id)
            .unwrap()
            .color
            .is_none()
    );

    let flow = f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Colored flow","purpose":"Display only","resource_kinds":["task"],"graph":{"states":[{"id":"todo","name":"Todo","initial":true,"color":"#aabbcc"}],"transitions":[{"id":"stay","name":"Stay","from":"todo","to":"todo"}]}})).unwrap()).unwrap();
    let version = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Initial display".into(),
            },
        )
        .unwrap();
    assert_eq!(
        version.graph.states[0].color.as_ref().map(HexColor::as_str),
        Some("#AABBCC")
    );
    let old_checksum = version.checksum.clone();
    let mut graph = version.graph.clone();
    graph.states[0].color = None;
    f.store.change_workflow(&f.ctx,&f.project.id,&flow.id,&serde_json::from_value(json!({"expected_revision":flow.revision+1,"graph":graph,"reason":"Default display"})).unwrap()).unwrap();
    let historical = f
        .store
        .workflow_version(&f.ctx, &f.project.id, &flow.id, &version.id)
        .unwrap();
    assert_eq!(historical.checksum, old_checksum);
    assert_eq!(
        historical.graph.states[0]
            .color
            .as_ref()
            .map(HexColor::as_str),
        Some("#AABBCC")
    );
}

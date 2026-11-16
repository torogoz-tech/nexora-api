use nexora_platform::{ActorKind, Capability, PlatformError, ProjectChange};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::{
    path::Path,
    sync::{Arc, Barrier},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires local PostgreSQL; revision conflicts, receipts, archives and scoped pagination"]
fn project_changes_are_atomic_versioned_and_replayed_without_overwriting_newer_changes() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_projects_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Project fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let first = store.create_project(&ctx, "Original", None).unwrap();
    let other = store.create_project(&ctx, "Other", None).unwrap();
    let third = store.create_project(&ctx, "Third", None).unwrap();
    let actor = store
        .create_actor(&ctx, ActorKind::Agent, "Scoped actor", "Project fixture")
        .unwrap();
    store
        .grant(&ctx, &actor.id, Capability::ProjectsRead, Some(&first.id))
        .unwrap();
    store
        .grant(&ctx, &actor.id, Capability::ProjectsRead, Some(&third.id))
        .unwrap();
    let key = store
        .issue_key(
            &ctx,
            &actor.id,
            "Project fixture",
            nexora_platform::KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![first.id.clone(), third.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = store.authenticate(&key.key).unwrap();
    let mut visited = vec![];
    let mut cursor = None;
    loop {
        let page = store
            .project_page(&reader, cursor.as_deref(), 1, false)
            .unwrap();
        visited.extend(page.items.iter().map(|p| p.id.clone()));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    visited.sort();
    let mut allowed = vec![first.id.clone(), third.id.clone()];
    allowed.sort();
    assert_eq!(visited, allowed);
    assert!(store.project_page(&ctx, None, 0, false).is_err());
    assert!(store.project_page(&ctx, None, 101, false).is_err());
    assert!(
        store
            .project_page(&ctx, Some("invalid cursor"), 1, false)
            .is_err()
    );
    let change = ProjectChange {
        expected_revision: 1,
        name: "Renamed".into(),
        description: Some("Architecture context".into()),
        archived: false,
        color: Default::default(),
    };
    assert!(matches!(
        store.change_project(&reader, &first.id, &change),
        Err(PlatformError::Forbidden)
    ));
    let input = json!({"id":first.id,"change":change});
    let changed = store
        .request(&ctx, "project.change", "change-once", &input, |s| {
            s.change_project(&ctx, &first.id, &change)
        })
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.name, "Renamed");
    assert!(matches!(
        store.change_project(&ctx, &first.id, &change),
        Err(PlatformError::Conflict(_))
    ));
    let gate = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ["Concurrent A", "Concurrent B"]
        .into_iter()
        .map(|name| {
            let c = config.clone();
            let n = namespace.clone();
            let ctx = ctx.clone();
            let id = first.id.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                let mut s = PlatformStore::connect(&c, &n).unwrap();
                gate.wait();
                s.change_project(
                    &ctx,
                    &id,
                    &ProjectChange {
                        expected_revision: 2,
                        name: name.into(),
                        description: None,
                        archived: false,
                        color: Default::default(),
                    },
                )
            })
        })
        .collect();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    let latest = store.get_project(&ctx, &first.id).unwrap();
    assert_eq!(latest.revision, 3);
    let replay = store
        .request(&ctx, "project.change", "change-once", &input, |s| {
            s.change_project(&ctx, &first.id, &change)
        })
        .unwrap();
    assert_eq!(replay.revision, 2);
    assert_eq!(store.get_project(&ctx, &first.id).unwrap().revision, 3);
    let archived = store
        .change_project(
            &ctx,
            &first.id,
            &ProjectChange {
                expected_revision: 3,
                name: latest.name.clone(),
                description: None,
                archived: true,
                color: Default::default(),
            },
        )
        .unwrap();
    assert!(archived.archived);
    assert_eq!(
        store
            .project_page(&reader, None, 100, false)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        store
            .project_page(&reader, None, 100, true)
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(matches!(
        store.get_project(&reader, &other.id),
        Err(PlatformError::NotFound)
    ));
    assert!(store.get_project(&ctx, &first.id).unwrap().archived);
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    let restored = store
        .change_project(
            &ctx,
            &first.id,
            &ProjectChange {
                expected_revision: 4,
                name: latest.name,
                description: None,
                archived: false,
                color: Default::default(),
            },
        )
        .unwrap();
    assert_eq!(restored.revision, 5);
    assert_eq!(
        store
            .project_page(&reader, None, 100, false)
            .unwrap()
            .items
            .len(),
        2
    );
}

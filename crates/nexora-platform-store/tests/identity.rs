use nexora_platform::{ActorKind, Capability, KeyScope};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires local PostgreSQL; isolated installation namespace and generated credentials"]
fn durable_identity_key_scopes_rotation_revocation_and_actor_suspension() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_test_{nonce}");
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let bootstrap = store
        .bootstrap("Public test organization", "owner", &password, "Test owner")
        .unwrap();
    assert!(
        store
            .bootstrap("Other organization", "other", &password, "Other")
            .is_err()
    );
    assert!(store.login("owner", "wrong").is_err());
    let session = store.login("OWNER", &password).unwrap();
    let owner = store.authenticate(&session.access_token).unwrap();
    assert_eq!(owner.actor_id(), bootstrap.owner.id);
    let project = store
        .create_project(&owner, "Public project", None)
        .unwrap();
    let second = store.create_project(&owner, "Other project", None).unwrap();
    let agent = store
        .create_actor(
            &owner,
            ActorKind::Agent,
            "External developer agent",
            "Public test",
        )
        .unwrap();
    store
        .grant(
            &owner,
            &agent.id,
            Capability::ProjectsRead,
            Some(&project.id),
        )
        .unwrap();
    let scope = KeyScope {
        capabilities: vec![Capability::ProjectsRead],
        project_ids: vec![project.id.clone()],
        all_projects: false,
        installation: false,
    };
    let issued = store
        .issue_key(&owner, &agent.id, "test client", scope, 3600)
        .unwrap();
    let principal = store.authenticate(&issued.key).unwrap();
    assert_eq!(principal.actor_id(), agent.id);
    assert!(
        principal
            .require(Capability::ProjectsRead, Some(&project.id))
            .is_ok()
    );
    assert!(
        principal
            .require(Capability::ProjectsRead, Some(&second.id))
            .is_err()
    );
    let visible = store.list_projects(&principal).unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, project.id);
    assert!(
        store
            .create_actor(&principal, ActorKind::Human, "spoof", "forbidden")
            .is_err()
    );
    let listed = store.list_keys(&owner, &agent.id).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(
        !serde_json::to_string(&listed)
            .unwrap()
            .contains(&issued.key)
    );
    let rotated = store.rotate_key(&owner, &issued.metadata.id, 3600).unwrap();
    assert!(store.authenticate(&issued.key).is_err());
    assert!(store.authenticate(&rotated.key).is_ok());
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    assert!(store.authenticate(&rotated.key).is_ok());
    store.revoke_key(&owner, &rotated.metadata.id).unwrap();
    assert!(store.authenticate(&rotated.key).is_err());
    let next = store
        .issue_key(
            &owner,
            &agent.id,
            "another",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.id],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    store.suspend_actor(&owner, &agent.id).unwrap();
    assert!(store.authenticate(&next.key).is_err());
    store.logout(&session.access_token).unwrap();
    assert!(store.authenticate(&session.access_token).is_err());
}

#[test]
#[ignore = "requires local PostgreSQL; races use one isolated installation"]
fn bootstrap_and_key_rotation_have_exactly_one_concurrent_winner() {
    use std::sync::{Arc, Barrier};
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_race_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let workers = (0..2)
        .map(|i| {
            let config = config.clone();
            let namespace = namespace.clone();
            let password = password.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut store = PlatformStore::connect(&config, &namespace).unwrap();
                barrier.wait();
                store
                    .bootstrap("One organization", &format!("owner{i}"), &password, "Owner")
                    .ok()
                    .map(|b| (format!("owner{i}"), b))
            })
        })
        .collect::<Vec<_>>();
    let winners = workers
        .into_iter()
        .filter_map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(winners.len(), 1);
    let session = store.login(&winners[0].0, &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let issued = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Concurrent rotation",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![],
                all_projects: true,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let workers = (0..2)
        .map(|_| {
            let config = config.clone();
            let namespace = namespace.clone();
            let ctx = ctx.clone();
            let id = issued.metadata.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut store = PlatformStore::connect(&config, &namespace).unwrap();
                barrier.wait();
                store.rotate_key(&ctx, &id, 3600).ok().map(|key| key.key)
            })
        })
        .collect::<Vec<_>>();
    let keys = workers
        .into_iter()
        .filter_map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 1);
    assert!(store.authenticate(&issued.key).is_err());
    assert!(store.authenticate(&keys[0]).is_ok());
    assert_eq!(
        store
            .list_keys(&ctx, ctx.actor_id())
            .unwrap()
            .iter()
            .filter(|k| k.revoked_at.is_none())
            .count(),
        1
    );
}

#[test]
#[ignore = "requires local PostgreSQL; verifies grants cannot be escalated or cached after revocation"]
fn grant_revocation_invalidates_previously_authenticated_context_and_escalation_is_denied() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_grants_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Grant test", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let owner = store.authenticate(&session.access_token).unwrap();
    let project = store
        .create_project(&owner, "Authorized project", None)
        .unwrap();
    let agent = store
        .create_actor(&owner, ActorKind::Agent, "Agent", "Read one project")
        .unwrap();
    store
        .grant(
            &owner,
            &agent.id,
            Capability::ProjectsRead,
            Some(&project.id),
        )
        .unwrap();
    assert!(
        store
            .issue_key(
                &owner,
                &agent.id,
                "Unauthorized global scope",
                KeyScope {
                    capabilities: vec![Capability::ProjectsRead],
                    project_ids: vec![],
                    all_projects: true,
                    installation: false
                },
                3600
            )
            .is_err()
    );
    let key = store
        .issue_key(
            &owner,
            &agent.id,
            "Scoped",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let agent_ctx = store.authenticate(&key.key).unwrap();
    assert!(
        store
            .grant(&agent_ctx, &agent.id, Capability::ActorsManage, None)
            .is_err()
    );
    assert!(store.get_project(&agent_ctx, &project.id).is_ok());
    store
        .revoke_grant(
            &owner,
            &agent.id,
            Capability::ProjectsRead,
            Some(&project.id),
        )
        .unwrap();
    assert!(store.get_project(&agent_ctx, &project.id).is_err());
    assert!(store.list_projects(&agent_ctx).unwrap().is_empty());
}

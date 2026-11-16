use nexora_platform::{
    ActorKind, Capability, GitProvider, KeyScope, PlatformError, RepositoryChange,
    RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires local PostgreSQL; repository drafts, scopes, pagination, restart and archive revisions"]
fn repository_registration_is_durable_scoped_and_does_not_claim_provider_authorization() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_repos_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = SecretGenerator::token("test_password").unwrap();
    store
        .bootstrap("Repository fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let owner = store.authenticate(&session.access_token).unwrap();
    let project = store
        .create_project(&owner, "Multi repository", None)
        .unwrap();
    let other = store
        .create_project(&owner, "Private project", None)
        .unwrap();
    let registration = RepositoryRegistration {
        provider: GitProvider::Github,
        full_name: "owner/backend".into(),
    };
    let input = json!({"project_id":project.id,"registration":registration});
    let first = store
        .request(&owner, "repository.register", "repo-once", &input, |s| {
            s.register_repository(&owner, &project.id, &registration)
        })
        .unwrap();
    let replay = store
        .request(&owner, "repository.register", "repo-once", &input, |s| {
            s.register_repository(&owner, &project.id, &registration)
        })
        .unwrap();
    assert_eq!(first.id, replay.id);
    assert!(first.id.starts_with("repo_"));
    assert_eq!(
        serde_json::to_value(&first).unwrap()["authorization_status"],
        "awaiting_authorization"
    );
    assert!(first.remote_id.is_none());
    assert!(first.connection_id.is_none());
    assert!(matches!(
        store.register_repository(&owner, &project.id, &registration),
        Err(PlatformError::Conflict(_))
    ));
    let second = store
        .register_repository(
            &owner,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "team/subgroup/frontend".into(),
            },
        )
        .unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(second.host.as_deref(), Some("gitlab.com"));
    let hidden = store
        .register_repository(&owner, &other.id, &registration)
        .unwrap();
    let actor = store
        .create_actor(&owner, ActorKind::Agent, "Reader", "Repository fixture")
        .unwrap();
    store
        .grant(
            &owner,
            &actor.id,
            Capability::RepositoriesRead,
            Some(&project.id),
        )
        .unwrap();
    let issued = store
        .issue_key(
            &owner,
            &actor.id,
            "Read repositories",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = store.authenticate(&issued.key).unwrap();
    let page = store
        .repository_page(&reader, &project.id, None, 1, false)
        .unwrap();
    assert_eq!(page.items.len(), 1);
    let next = store
        .repository_page(&reader, &project.id, page.next_cursor.as_deref(), 1, false)
        .unwrap();
    assert_eq!(next.items.len(), 1);
    assert!(next.next_cursor.is_none());
    assert_ne!(page.items[0].id, next.items[0].id);
    assert!(matches!(
        store.get_repository(&reader, &other.id, &hidden.id),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        store.get_repository(&reader, &project.id, &hidden.id),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        store.register_repository(&reader, &project.id, &registration),
        Err(PlatformError::Forbidden)
    ));
    assert!(
        store
            .repository_page(&reader, &project.id, None, 101, false)
            .is_err()
    );
    assert!(
        store
            .repository_page(&reader, &project.id, Some("invalid"), 1, false)
            .is_err()
    );
    let archived = store
        .change_repository(
            &owner,
            &project.id,
            &first.id,
            &RepositoryChange {
                expected_revision: 1,
                archived: true,
            },
        )
        .unwrap();
    assert_eq!(archived.revision, 2);
    assert!(matches!(
        store.change_repository(
            &owner,
            &project.id,
            &first.id,
            &RepositoryChange {
                expected_revision: 1,
                archived: false
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        store
            .repository_page(&reader, &project.id, None, 100, false)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        store
            .repository_page(&reader, &project.id, None, 100, true)
            .unwrap()
            .items
            .len(),
        2
    );
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    assert!(store.ready().unwrap());
    let restored = store
        .change_repository(
            &owner,
            &project.id,
            &first.id,
            &RepositoryChange {
                expected_revision: 2,
                archived: false,
            },
        )
        .unwrap();
    assert_eq!(restored.id, first.id);
    assert_eq!(restored.revision, 3);
    assert!(restored.remote_id.is_none());
    store
        .revoke_grant(
            &owner,
            &actor.id,
            Capability::RepositoriesRead,
            Some(&project.id),
        )
        .unwrap();
    assert!(
        store
            .repository_page(&reader, &project.id, None, 100, false)
            .is_err()
    );
    for bad in [
        "../../private",
        "owner/repo?token=x",
        "https://github.com/owner/repo",
        "owner/repo/extra",
        "owner/repo\n",
    ] {
        assert!(
            store
                .register_repository(
                    &owner,
                    &project.id,
                    &RepositoryRegistration {
                        provider: GitProvider::Github,
                        full_name: bad.into()
                    }
                )
                .is_err()
        );
    }
}

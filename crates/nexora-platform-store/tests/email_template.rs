#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; verifies template persistence, authority and optimistic concurrency"]
fn organization_template_is_shared_guarded_and_revisioned() {
    let mut f = WorkItemFixture::new();
    let initial = f.store.email_template(&f.ctx).unwrap();
    assert_eq!(initial.html, EmailTemplate::DEFAULT_HTML);
    let change = EmailTemplateChange {
        expected_revision: initial.revision,
        html: "<h1>{{organization_name}}</h1><p>{{body}}</p>".into(),
    };
    let saved = f.store.change_email_template(&f.ctx, &change).unwrap();
    assert_eq!(saved.revision, initial.revision + 1);
    assert_eq!(f.store.email_template(&f.ctx).unwrap().html, change.html);
    assert!(matches!(
        f.store.change_email_template(&f.ctx, &change),
        Err(PlatformError::Conflict(_))
    ));
    let limited = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "No settings",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = f.store.authenticate(&limited.key).unwrap();
    assert!(f.store.email_template(&reader).is_err());
    assert!(
        f.store
            .change_email_template(
                &reader,
                &EmailTemplateChange {
                    expected_revision: saved.revision,
                    html: "{{body}}".into()
                }
            )
            .is_err()
    );
    let (delivery, organization) = f.store.email_template_for_delivery().unwrap();
    assert_eq!(delivery.html, saved.html);
    assert!(!organization.is_empty());
}

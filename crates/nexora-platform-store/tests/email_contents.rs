#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; bilingual email catalog, independent revisions and customization"]
fn email_content_is_bilingual_guarded_independently_revisioned_and_restorable() {
    let mut f = WorkItemFixture::new();
    let rows = f.store.email_contents(&f.ctx).unwrap();
    assert_eq!(rows.len(), 20);
    for row in &rows {
        row.validate(&row.subject, &row.body_html).unwrap();
        assert!(!row.customized);
    }
    let spanish = f.store.email_content(&f.ctx, "test", "es").unwrap();
    let english = f.store.email_content(&f.ctx, "test", "en").unwrap();
    let change = EmailContentChange {
        expected_revision: spanish.revision,
        subject: "Correo de prueba".into(),
        body_html: "<p>{{organization_name}} funciona.</p>".into(),
    };
    let saved = f
        .store
        .change_email_content(&f.ctx, "test", "es", &change)
        .unwrap();
    assert!(saved.customized);
    let mail = nexora_platform_store::EmailNotification {
        event_key: "test-preview".into(),
        kind: "test".into(),
        project_id: None,
        resource_id: "smtp-test".into(),
        actor_id: f.ctx.actor_id().to_owned(),
        email: "recipient@example.com".into(),
        attempts: 1,
        language: "es".into(),
    };
    let (subject, body) = f
        .store
        .email_content_for_delivery(&mail, "<Organization>")
        .unwrap();
    assert_eq!(subject, "Correo de prueba");
    assert!(body.contains("&lt;Organization&gt; funciona."));
    assert_eq!(saved.revision, spanish.revision + 1);
    assert_eq!(
        f.store.email_content(&f.ctx, "test", "en").unwrap().subject,
        english.subject
    );
    assert!(matches!(
        f.store.change_email_content(&f.ctx, "test", "es", &change),
        Err(PlatformError::Conflict(_))
    ));
    let restored = f
        .store
        .change_email_content(
            &f.ctx,
            "test",
            "es",
            &EmailContentChange {
                expected_revision: saved.revision,
                subject: saved.default_subject,
                body_html: saved.default_body_html,
            },
        )
        .unwrap();
    assert!(!restored.customized);
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
    assert!(f.store.email_contents(&reader).is_err());
    assert!(
        f.store
            .change_email_content(
                &reader,
                "test",
                "en",
                &EmailContentChange {
                    expected_revision: english.revision,
                    subject: "No authority".into(),
                    body_html: "Message".into()
                }
            )
            .is_err()
    );
}

#[path = "support/email_flow_fixture.rs"]
mod email_flow_fixture;
use email_flow_fixture::EmailFlowFixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::json;
#[test]
#[ignore = "requires PostgreSQL; invitation delivery, acceptance and confirmation without sending mail"]
fn invitation_join_and_confirmation_use_single_use_protected_links() {
    let mut f = EmailFlowFixture::new();
    let w = &mut f.work;
    let invite = w
        .store
        .create_member_invitation(
            &w.ctx,
            &MemberInvitationCreate {
                email: "ana@example.invalid".into(),
                name: "Ana".into(),
            },
        )
        .unwrap();
    assert!(!serde_json::to_string(&invite).unwrap().contains("nxi_"));
    let (mail, token) = f.claim_token("invitation");
    assert!(f.work.store.inspect_member_invitation(&token).is_ok());
    let encrypted: bool = f
        .sql
        .query_one(
            "SELECT position($1 in token_secret::text)=0 FROM member_invitations WHERE id=$2",
            &[&token.as_str(), &invite.id],
        )
        .unwrap()
        .get(0);
    assert!(encrypted);
    f.work
        .store
        .finish_email_notification(&mail.event_key, mail.attempts, "sent")
        .unwrap();
    assert!(f.work.store.inspect_member_invitation(&token).is_ok());
    let password = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
    let input = MemberInvitationAccept {
        token: token.to_string(),
        username: "ana".into(),
        password: password.to_string(),
        name: "Ana".into(),
    };
    f.work.store.accept_member_invitation(&input).unwrap();
    assert!(f.work.store.accept_member_invitation(&input).is_err());
    assert!(f.work.store.inspect_member_invitation(&token).is_err());
    let session = f.work.store.login("ana", &password).unwrap();
    let member = f.work.store.authenticate(&session.access_token).unwrap();
    assert!(member.require(Capability::ProjectsRead, None).is_err());
    let notice = f
        .work
        .store
        .claim_email_notification(true)
        .unwrap()
        .unwrap();
    assert_eq!(notice.kind, "invitation_accepted");
    assert_eq!(notice.email, "owner@example.invalid");
    let (_, body) = f
        .work
        .store
        .email_content_for_delivery(&notice, "Org")
        .unwrap();
    assert!(body.contains("Ana"));
    assert_eq!(
        f.work
            .store
            .organization_members(&f.work.ctx)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
#[ignore = "requires PostgreSQL; resend rotation, cancellation, expiry and loss of inviter authority"]
fn invitation_resend_revoke_and_expiry_invalidate_old_links() {
    let mut f = EmailFlowFixture::new();
    let w = &mut f.work;
    let invite = w
        .store
        .create_member_invitation(
            &w.ctx,
            &MemberInvitationCreate {
                email: "join@example.invalid".into(),
                name: "Join".into(),
            },
        )
        .unwrap();
    let (_, old) = f.claim_token("invitation");
    let resent = f
        .work
        .store
        .change_member_invitation(
            &f.work.ctx,
            &invite.id,
            &MemberInvitationChange {
                expected_revision: invite.revision,
            },
            true,
        )
        .unwrap();
    assert!(f.work.store.inspect_member_invitation(&old).is_err());
    let (_, new) = f.claim_token("invitation");
    assert!(f.work.store.inspect_member_invitation(&new).is_ok());
    f.sql.execute("UPDATE member_invitations SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",&[&invite.id]).unwrap();
    assert!(f.work.store.inspect_member_invitation(&new).is_err());
    let again = f
        .work
        .store
        .change_member_invitation(
            &f.work.ctx,
            &invite.id,
            &MemberInvitationChange {
                expected_revision: resent.revision,
            },
            true,
        )
        .unwrap();
    let (_, active) = f.claim_token("invitation");
    f.work
        .store
        .change_member_invitation(
            &f.work.ctx,
            &invite.id,
            &MemberInvitationChange {
                expected_revision: again.revision,
            },
            false,
        )
        .unwrap();
    assert!(f.work.store.inspect_member_invitation(&active).is_err());
    assert!(
        f.work
            .store
            .claim_email_notification(true)
            .unwrap()
            .is_none()
    );
}
#[test]
#[ignore = "requires PostgreSQL; password recovery, anti-enumeration, revocation, replay and OIDC-only exclusion"]
fn recovery_revokes_sessions_and_never_creates_an_oidc_password() {
    let mut f = EmailFlowFixture::new();
    f.work
        .store
        .request_password_recovery("missing@example.invalid")
        .unwrap();
    let none: i64 = f
        .sql
        .query_one("SELECT count(*) FROM password_reset_requests", &[])
        .unwrap()
        .get(0);
    assert_eq!(none, 0);
    let old = zeroize::Zeroizing::new(SecretGenerator::token("old_password").unwrap());
    let new = zeroize::Zeroizing::new(SecretGenerator::token("new_password").unwrap());
    let actor = f
        .work
        .store
        .create_user(&f.work.ctx, "recover", &old, "Recover")
        .unwrap();
    f.work
        .store
        .change_notification_recipient(
            &f.work.ctx,
            &actor.id,
            &NotificationRecipientChange {
                email: "recover@example.invalid".into(),
                assignments: false,
                approvals: false,
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.work.store.login("recover", &old).unwrap();
    f.work
        .store
        .request_password_recovery("recover@example.invalid")
        .unwrap();
    let (mail, token) = f.claim_token("password_reset");
    f.work
        .store
        .finish_email_notification(&mail.event_key, mail.attempts, "sent")
        .unwrap();
    f.work
        .store
        .complete_password_recovery(&token, &new)
        .unwrap();
    assert!(
        f.work
            .store
            .complete_password_recovery(&token, &new)
            .is_err()
    );
    assert!(f.work.store.login("recover", &old).is_err());
    assert!(f.work.store.authenticate(&session.access_token).is_err());
    assert!(f.work.store.login("recover", &new).is_ok());
    f.sql
        .execute(
            "UPDATE users SET password_hash=NULL WHERE actor_id=$1",
            &[&actor.id],
        )
        .unwrap();
    f.work
        .store
        .request_password_recovery("recover@example.invalid")
        .unwrap();
    assert!(
        f.work
            .store
            .claim_email_notification(true)
            .unwrap()
            .is_none()
    );
}
#[test]
#[ignore = "requires PostgreSQL; reset rotation, expiry, current-address binding and bounded requests"]
fn recovery_tokens_rotate_and_expire_and_are_rate_limited() {
    let mut f = EmailFlowFixture::new();
    f.work
        .store
        .request_password_recovery("owner@example.invalid")
        .unwrap();
    let (_, old) = f.claim_token("password_reset");
    f.work
        .store
        .request_password_recovery("owner@example.invalid")
        .unwrap();
    let (_, new) = f.claim_token("password_reset");
    let password = zeroize::Zeroizing::new(SecretGenerator::token("next_password").unwrap());
    assert!(
        f.work
            .store
            .complete_password_recovery(&old, &password)
            .is_err()
    );
    f.sql.execute("UPDATE password_reset_requests SET expires_at=clock_timestamp()-interval '1 second' WHERE used_at IS NULL",&[]).unwrap();
    assert!(
        f.work
            .store
            .complete_password_recovery(&new, &password)
            .is_err()
    );
    for _ in 0..8 {
        f.work
            .store
            .request_password_recovery("owner@example.invalid")
            .unwrap();
    }
    let count: i64 = f
        .sql
        .query_one("SELECT count(*) FROM password_reset_requests", &[])
        .unwrap()
        .get(0);
    assert_eq!(count, 3);
}
#[test]
#[ignore = "requires PostgreSQL; approval outcomes notify the requester with current read authority"]
fn approval_decision_notifies_requester_and_respects_current_permissions() {
    let mut f = EmailFlowFixture::new();
    let w = &mut f.work;
    let flow=w.store.create_workflow(&w.ctx,&w.project.id,&serde_json::from_value(json!({"name":"Outcome","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let version = w
        .store
        .publish_workflow(
            &w.ctx,
            &w.project.id,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Publish".into(),
            },
        )
        .unwrap();
    let task=w.store.create_task(&w.ctx,&w.project.id,&serde_json::from_value(json!({"title":"Review","type_code":"task","workflow_version_id":version.id,"origin":"fixture"})).unwrap()).unwrap();
    let request=w.store.create_approval_request(&w.ctx,&w.project.id,&serde_json::from_value(json!({"target":{"kind":"task","task_id":task.id,"transition_id":"finish","parameters":{"expected_revision":task.revision,"reason":"Review"}},"reason":"Request","expires_in_seconds":3600})).unwrap()).unwrap();
    let initial = w.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(initial.kind, "approval");
    w.store
        .finish_email_notification(&initial.event_key, initial.attempts, "sent")
        .unwrap();
    w.store
        .decide_approval_request(
            &w.ctx,
            &w.project.id,
            &request.id,
            &ApprovalRequestDecide {
                expected_revision: request.revision,
                decision: ApprovalDecision::Rejected,
                reason: "<script>Rejected</script>".into(),
                approval_expires_in_seconds: 3600,
            },
        )
        .unwrap();
    let result = w.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(result.kind, "approval_result");
    assert_eq!(result.actor_id, w.ctx.actor_id());
    let (_, body) = w.store.email_content_for_delivery(&result, "Org").unwrap();
    assert!(body.contains("Rechazada"));
    assert!(!body.contains("<script>"));
    w.store
        .finish_email_notification(&result.event_key, result.attempts, "sent")
        .unwrap();
    let next=w.store.create_approval_request(&w.ctx,&w.project.id,&serde_json::from_value(json!({"target":{"kind":"task","task_id":task.id,"transition_id":"finish","parameters":{"expected_revision":task.revision,"reason":"Review"}},"reason":"Request again","expires_in_seconds":3600})).unwrap()).unwrap();
    w.store
        .decide_approval_request(
            &w.ctx,
            &w.project.id,
            &next.id,
            &ApprovalRequestDecide {
                expected_revision: next.revision,
                decision: ApprovalDecision::Approved,
                reason: "Approved after revision".into(),
                approval_expires_in_seconds: 3600,
            },
        )
        .unwrap();
    assert!(w.store.claim_email_notification(true).unwrap().is_none()); // Already-decided request notice is skipped.
    let approved = w.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(approved.kind, "approval_result");
    let (_, body) = w
        .store
        .email_content_for_delivery(&approved, "Org")
        .unwrap();
    assert!(body.contains("Aprobada"));
    w.store
        .finish_email_notification(&approved.event_key, approved.attempts, "pending")
        .unwrap();
    let owner = w.ctx.actor_id().to_owned();
    w.store
        .revoke_grant(&w.ctx, &owner, Capability::TasksRead, None)
        .unwrap();
    f.sql
        .execute(
            "UPDATE email_notifications SET available_at=clock_timestamp() WHERE event_key=$1",
            &[&approved.event_key],
        )
        .unwrap();
    assert!(w.store.claim_email_notification(true).unwrap().is_none());
}
#[test]
#[ignore = "requires PostgreSQL; delivery and redemption use current authority and current email"]
fn email_actions_recheck_inviter_authority_and_recovery_address() {
    let mut f = EmailFlowFixture::new();
    f.work
        .store
        .create_member_invitation(
            &f.work.ctx,
            &MemberInvitationCreate {
                email: "bound@example.invalid".into(),
                name: "Bound".into(),
            },
        )
        .unwrap();
    let (_, invite_token) = f.claim_token("invitation");
    f.sql
        .execute(
            "DELETE FROM actor_grants WHERE actor_id=$1 AND capability='actors:manage'",
            &[&f.work.ctx.actor_id()],
        )
        .unwrap();
    assert!(
        f.work
            .store
            .inspect_member_invitation(&invite_token)
            .is_err()
    );
    let password = zeroize::Zeroizing::new(SecretGenerator::token("next_password").unwrap());
    assert!(
        f.work
            .store
            .accept_member_invitation(&MemberInvitationAccept {
                token: invite_token.to_string(),
                username: "bound".into(),
                name: "Bound".into(),
                password: password.to_string(),
            })
            .is_err()
    );
    f.work
        .store
        .request_password_recovery("owner@example.invalid")
        .unwrap();
    let (_, reset_token) = f.claim_token("password_reset");
    f.sql
        .execute(
            "UPDATE notification_recipients SET email='changed@example.invalid' WHERE actor_id=$1",
            &[&f.work.ctx.actor_id()],
        )
        .unwrap();
    assert!(
        f.work
            .store
            .complete_password_recovery(&reset_token, &password)
            .is_err()
    );
}

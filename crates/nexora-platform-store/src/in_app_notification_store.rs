use crate::{
    InAppNotificationChange, InAppNotificationQuery, PlatformStore,
    labor_calendar_policy::LaborCalendarPolicy, task_read_authority::TaskReadAuthority,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};

impl PlatformStore {
    pub fn in_app_notifications(
        &mut self,
        ctx: &AuthContext,
        query: &InAppNotificationQuery,
    ) -> Result<Value, PlatformError> {
        if !["inbox", "unread", "archived"].contains(&query.folder.as_str())
            || !(1..=100).contains(&query.limit)
            || query.offset > 100_000
        {
            return Err(PlatformError::Validation("invalid notification page"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        let scope = TaskReadAuthority::scope(&mut tx, ctx)?;
        let projectless =
            match TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksRead, None) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(error) => return Err(error),
            };
        let spanish: bool = tx
            .query_one(
                "SELECT language='es' FROM installation_preferences WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        let visibility = include_str!("in_app_notification_visibility.sql");
        let filter = "(($5='inbox' AND n.archived_at IS NULL) OR ($5='unread' AND n.archived_at IS NULL AND n.read_at IS NULL) OR ($5='archived' AND n.archived_at IS NOT NULL))";
        let count = tx.query_one(&format!("SELECT count(*) FILTER(WHERE n.read_at IS NULL AND n.archived_at IS NULL), count(*) FILTER(WHERE {filter}) FROM in_app_notifications n WHERE {visibility}"), &[&ctx.actor_id(), &scope.projects, &scope.unrestricted, &projectless, &query.folder]).map_err(Self::database_error)?;
        let projection = include_str!("in_app_notification_projection.sql");
        let rows = tx.query(&format!("SELECT {projection} FROM in_app_notifications n WHERE {visibility} AND {filter} ORDER BY n.created_at DESC,n.id DESC LIMIT $6 OFFSET $7"), &[&ctx.actor_id(), &scope.projects, &scope.unrestricted, &projectless, &query.folder, &i64::from(query.limit), &i64::from(query.offset)]).map_err(Self::database_error)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut value: Value = row.get(0);
            let kind_owned = value["kind"].as_str().unwrap_or_default().to_owned();
            let kind = kind_owned.as_str();
            value["title"] = json!(crate::in_app_notification_title::in_app_notification_title(
                kind, spanish
            ));
            value["body"] = json!(crate::in_app_notification_body::in_app_notification_body(
                &value, spanish
            ));
            let resource_owned = value["resource_id"].as_str().unwrap_or_default().to_owned();
            let resource = resource_owned.as_str();
            let project = value["project_id"].as_str().unwrap_or_default().to_owned();
            value["url"] = if matches!(kind, "assignment" | "task_comment" | "task_updated") {
                json!(format!("/tasks/{resource}"))
            } else if kind.starts_with("calendar_") {
                if kind == "calendar_cancelled" {
                    json!("/calendar")
                } else {
                    json!(format!("/calendar/activities/{resource}"))
                }
            } else if kind.starts_with("overtime_") {
                json!("/overtime")
            } else if kind.starts_with("leave_") || kind.starts_with("holiday_") {
                json!("/calendar")
            } else if kind == "workflow_assignment" {
                json!("/tasks")
            } else if kind == "access_request" {
                json!("/settings/members")
            } else if matches!(kind, "role_changed" | "security_changed") {
                json!("/profile")
            } else if kind.starts_with("review_") || kind == "repository_failed" {
                json!(format!("/code/repositories/{project}/{resource}"))
            } else if matches!(kind, "knowledge_review" | "knowledge_result") {
                json!("/approvals")
            } else if kind.starts_with("feature_") {
                json!(format!("/knowledge/features?project={project}"))
            } else {
                tx.query_opt(
                    "SELECT resource_id FROM approval_requests WHERE id=$1 AND target_kind='task'",
                    &[&resource],
                )
                .map_err(Self::database_error)?
                .map(|r| json!(format!("/tasks/{}", r.get::<_, String>(0))))
                .unwrap_or_else(|| json!(format!("/knowledge?project={project}")))
            };
            items.push(value);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(
            json!({"items":items,"unread_count":count.get::<_,i64>(0),"total":count.get::<_,i64>(1)}),
        )
    }

    pub fn change_in_app_notification(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        change: &InAppNotificationChange,
    ) -> Result<Value, PlatformError> {
        if id.is_empty() || id.len() > 512 || change.read.is_none() && change.archived.is_none() {
            return Err(PlatformError::Validation("invalid notification change"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        // State contains no resource content; ownership is sufficient to dismiss a notification
        // whose underlying permission was revoked. Such notifications never appear in GET/count.
        let row = tx.query_opt("UPDATE in_app_notifications SET read_at=CASE WHEN $3::boolean IS NULL THEN read_at WHEN $3 THEN COALESCE(read_at,clock_timestamp()) ELSE NULL END,archived_at=CASE WHEN $4::boolean IS NULL THEN archived_at WHEN $4 THEN COALESCE(archived_at,clock_timestamp()) ELSE NULL END WHERE id=$1 AND recipient_id=$2 RETURNING jsonb_build_object('id',id,'read_at',read_at,'archived_at',archived_at)", &[&id,&ctx.actor_id(),&change.read,&change.archived]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(row.get(0))
    }

    pub fn read_all_in_app_notifications(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        let scope = TaskReadAuthority::scope(&mut tx, ctx)?;
        let projectless =
            match TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksRead, None) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(error) => return Err(error),
            };
        let visibility = include_str!("in_app_notification_visibility.sql");
        let updated = tx.execute(&format!("UPDATE in_app_notifications n SET read_at=clock_timestamp() WHERE {visibility} AND n.read_at IS NULL AND n.archived_at IS NULL"), &[&ctx.actor_id(), &scope.projects, &scope.unrestricted, &projectless]).map_err(Self::database_error)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"updated":updated}))
    }
}

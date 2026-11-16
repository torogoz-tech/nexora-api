use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    labor_calendar_policy::LaborCalendarPolicy, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

impl PlatformStore {
    pub fn rating_directory(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<Value, PlatformError> {
        let limit = i64::from(limit.clamp(1, 100));
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        let admin = Self::rating_admin(&mut tx, ctx.actor_id())?;
        let lead = Self::rating_lead(&mut tx, ctx.actor_id())?;
        let rows = tx.query("SELECT a.id,a.name FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.active AND a.kind='human' AND r.role IN ('developer','technical_lead') AND ($2::text IS NULL OR a.id>$2) AND ($3 OR a.id=$1 OR EXISTS(SELECT 1 FROM actor_roles me JOIN projects p ON NOT p.archived WHERE me.actor_id=$1 AND me.role IN ('developer','technical_lead','pmo','manager','super_admin') AND (me.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=$1 AND mp.project_id=p.id)) AND (r.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=a.id AND mp.project_id=p.id)))) ORDER BY a.id LIMIT $4", &[&ctx.actor_id(),&after,&admin,&(limit+1)]).map_err(Self::database_error)?;
        let mut items = Vec::new();
        for row in rows.iter().take(limit as usize) {
            let id: String = row.get(0);
            let shared = Self::rating_projects(&mut tx, ctx.actor_id(), &id)?;
            let projects: Vec<Value> = tx.query("SELECT id,name FROM projects WHERE id=ANY($1) AND NOT archived ORDER BY name,id", &[&shared]).map_err(Self::database_error)?.iter().map(|p| json!({"id":p.get::<_,String>(0),"name":p.get::<_,String>(1)})).collect();
            items.push(json!({"id":id,"name":row.get::<_,String>(1),"projects":projects,"can_view_evaluations":(admin || (lead && !shared.is_empty())) && id!=ctx.actor_id(),"can_rate":id!=ctx.actor_id()}));
        }
        let has_more = rows.len() > limit as usize;
        let next_after = if has_more {
            items
                .last()
                .and_then(|i| i["id"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"has_more":has_more,"next_after":next_after}))
    }
    /// Rebuild activity baselines after midnight and after retroactive calendar changes.
    /// Exclusions are evaluated live by reports, retaining original evidence for audit.
    pub fn refresh_developer_ratings(&mut self) -> Result<u64, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        // Serialize workers and lock calendar policy while consuming its dirty flags.
        tx.query(
            "SELECT singleton FROM labor_calendar_settings FOR UPDATE",
            &[],
        )
        .map_err(Self::database_error)?;
        let dirty: Vec<String> = tx
            .query(
                "SELECT actor_id FROM developer_rating_dirty FOR UPDATE",
                &[],
            )
            .map_err(Self::database_error)?
            .iter()
            .map(|r| r.get(0))
            .collect();
        let changed = tx
            .query_one(include_str!("developer_rating_refresh.sql"), &[])
            .map_err(Self::database_error)?
            .get::<_, i64>(0) as u64;
        tx.execute("INSERT INTO developer_rating_closures(actor_id,last_closed,calendar_revision) SELECT r.actor_id,(transaction_timestamp() AT TIME ZONE s.timezone)::date-1,s.revision FROM actor_roles r CROSS JOIN labor_calendar_settings s WHERE r.role IN ('developer','technical_lead') ON CONFLICT(actor_id) DO UPDATE SET last_closed=EXCLUDED.last_closed,calendar_revision=EXCLUDED.calendar_revision",&[]).map_err(Self::database_error)?;
        tx.execute(
            "DELETE FROM developer_rating_dirty WHERE actor_id=ANY($1)",
            &[&dirty],
        )
        .map_err(Self::database_error)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(changed)
    }
    pub fn member_ratings(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        year: i32,
    ) -> Result<Value, PlatformError> {
        if !(2000..=9998).contains(&year) {
            return Err(PlatformError::Validation("invalid ratings year"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        if ctx.actor_id() == actor {
            return Err(PlatformError::Forbidden);
        }
        let admin = Self::rating_admin(&mut tx, ctx.actor_id())?;
        let lead = Self::rating_lead(&mut tx, ctx.actor_id())?;
        if !admin && !lead {
            return Err(PlatformError::Forbidden);
        }
        Self::rating_target(&mut tx, actor)?;
        let projects = Self::rating_projects(&mut tx, ctx.actor_id(), actor)?;
        if !admin && projects.is_empty() {
            return Err(PlatformError::Forbidden);
        }
        if ctx.actor_id() != actor {
            if let Some(project) = projects.first() {
                PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, Some(project))?;
            } else {
                PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)?;
            }
        }
        let mut value: Value = tx
            .query_one(
                include_str!("developer_rating_report.sql"),
                &[&actor, &year],
            )
            .map_err(Self::database_error)?
            .get(0);
        let admin = Self::rating_admin(&mut tx, ctx.actor_id())?;
        if ctx.actor_id() != actor
            && !admin
            && let Some(days) = value["daily"].as_array_mut()
        {
            for day in days {
                if let Some(entries) = day["entries"].as_array_mut() {
                    entries.retain(|entry| {
                        entry["source"] == "activity"
                            || entry["project_id"].is_null()
                            || entry["project_id"]
                                .as_str()
                                .is_some_and(|id| projects.iter().any(|p| p == id))
                    });
                }
            }
        }
        value["can_rate"] = json!(ctx.actor_id() != actor);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn create_member_rating(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let object = input
            .as_object()
            .ok_or(PlatformError::Validation("invalid rating"))?;
        if object.keys().any(|k| {
            !matches!(
                k.as_str(),
                "project_id" | "score" | "reason" | "evidence_url" | "evidence_ids" | "date"
            )
        }) {
            return Err(PlatformError::Validation("unknown rating field"));
        }
        let project = match input.get("project_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.is_empty() => None,
            Some(Value::String(value)) => Some(value.as_str()),
            _ => return Err(PlatformError::Validation("invalid project")),
        };
        let score = input["score"]
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=5.0).contains(v))
            .ok_or(PlatformError::Validation("score must be between 0 and 5"))?;
        let reason = input["reason"]
            .as_str()
            .map(str::trim)
            .filter(|v| !v.is_empty() && v.len() <= 4000)
            .ok_or(PlatformError::Validation("rating reason is required"))?;
        let evidence = match input.get("evidence_url") {
            None | Some(Value::Null) => None,
            Some(Value::String(v))
                if v.len() <= 2048 && (v.starts_with("https://") || v.starts_with("http://")) =>
            {
                let url = url::Url::parse(v)
                    .map_err(|_| PlatformError::Validation("invalid evidence URL"))?;
                if url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err(PlatformError::Validation("invalid evidence URL"));
                }
                Some(v.as_str())
            }
            _ => return Err(PlatformError::Validation("invalid evidence URL")),
        };
        let day = input["date"]
            .as_str()
            .filter(|v| v.len() == 10)
            .ok_or(PlatformError::Validation("date is required"))?;
        let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
            .map_err(|_| PlatformError::Storage)?;
        time::Date::parse(day, &format)
            .map_err(|_| PlatformError::Validation("invalid rating date"))?;
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session
            || ctx.kind() != ActorKind::Human
            || ctx.actor_id() == actor
        {
            return Err(PlatformError::Forbidden);
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, &ctx)?;
        Self::rating_target(&mut tx, actor)?;
        // Lock current role and project assignments; stale authenticated grants cannot rate.
        let projects = Self::rating_projects(&mut tx, ctx.actor_id(), actor)?;
        if let Some(project) = project {
            if !projects.iter().any(|p| p == project) {
                return Err(PlatformError::Forbidden);
            }
            PlatformAuthority::require(&mut tx, &ctx, Capability::ProjectsRead, Some(project))?;
        }
        Self::rating_general_authority(&mut tx, ctx.actor_id())?;
        let role: String = tx
            .query_one(
                "SELECT role FROM actor_roles WHERE actor_id=$1 FOR SHARE",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .get(0);
        let source = if matches!(role.as_str(), "developer" | "technical_lead") {
            "peer"
        } else {
            "manager"
        };
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "member.rating.create",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let allowed:bool=tx.query_one("SELECT $2::text::date>=GREATEST(s.starts_on,(r.rating_started_at AT TIME ZONE s.timezone)::date,(a.created_at AT TIME ZONE s.timezone)::date) AND $2::text::date<=(clock_timestamp() AT TIME ZONE s.timezone)::date AND NOT developer_rating_excluded(a.id,$2::text::date) FROM actors a JOIN actor_roles r ON r.actor_id=a.id CROSS JOIN labor_calendar_settings s WHERE a.id=$1",&[&actor,&day]).map_err(Self::database_error)?.get(0);
        if !allowed {
            return Err(PlatformError::Validation(
                "rating date is outside an eligible workday",
            ));
        }
        let mut files: Vec<String> = input
            .get("evidence_ids")
            .map(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|_| PlatformError::Validation("invalid evidence ids"))
            })
            .transpose()?
            .unwrap_or_default();
        if files.len() > 20
            || files
                .iter()
                .any(|id| !id.starts_with("rev_") || id.len() > 128)
            || files
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != files.len()
        {
            return Err(PlatformError::Validation("invalid evidence ids"));
        }
        files.sort();
        for file in &files {
            tx.query_opt("SELECT id FROM rating_evidence WHERE id=$1 AND uploaded_by=$2 AND target_id=$3 AND project_id IS NOT DISTINCT FROM $4 AND rating_id IS NULL AND created_at>clock_timestamp()-interval '24 hours' FOR UPDATE",&[&file,&ctx.actor_id(),&actor,&project]).map_err(Self::database_error)?.ok_or(PlatformError::Forbidden)?;
        }
        let id = SecretGenerator::id("rating")?;
        let value:Value=tx.query_one("INSERT INTO developer_rating_entries AS e(id,actor_id,date,source,score,reason,evidence_url,created_by,project_id) VALUES($1,$2,$3::text::date,$4,$5,$6,$7,$8,$9) RETURNING to_jsonb(e)-'actor_id'-'date'",&[&id,&actor,&day,&source,&score,&reason,&evidence,&ctx.actor_id(),&project]).map_err(Self::database_error)?.get(0);
        tx.execute(
            "UPDATE rating_evidence SET rating_id=$1 WHERE id=ANY($2)",
            &[&id, &files],
        )
        .map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "member.rating.create",
            Some(actor),
            json!({"entry":value,"date":day}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub(crate) fn rating_lead(
        tx: &mut postgres::Transaction<'_>,
        actor: &str,
    ) -> Result<bool, PlatformError> {
        Ok(tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role='technical_lead' FOR SHARE",&[&actor]).map_err(Self::database_error)?.is_some())
    }
    pub(crate) fn rating_admin(
        tx: &mut postgres::Transaction<'_>,
        actor: &str,
    ) -> Result<bool, PlatformError> {
        Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.id=$1 AND a.active AND r.role IN ('super_admin','manager'))",&[&actor]).map_err(Self::database_error)?.get(0))
    }
    pub(crate) fn rating_target(
        tx: &mut postgres::Transaction<'_>,
        actor: &str,
    ) -> Result<(), PlatformError> {
        // Match member-role mutations: acquire the actor before its role row.
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active AND kind='human' FOR SHARE",
            &[&actor],
        )
        .map_err(Self::database_error)?
        .ok_or(PlatformError::NotFound)?;
        tx.query_opt(
            "SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('developer','technical_lead') FOR SHARE",
            &[&actor],
        )
        .map_err(Self::database_error)?
        .ok_or(PlatformError::NotFound)?;
        Ok(())
    }
    pub(crate) fn rating_projects(
        tx: &mut postgres::Transaction<'_>,
        rater: &str,
        target: &str,
    ) -> Result<Vec<String>, PlatformError> {
        tx.query(
            "SELECT actor_id FROM actor_roles WHERE actor_id=ANY($1) ORDER BY actor_id FOR SHARE",
            &[&vec![rater, target]],
        )
        .map_err(Self::database_error)?;
        tx.query("SELECT actor_id FROM member_role_projects WHERE actor_id=ANY($1) ORDER BY actor_id,project_id FOR SHARE",&[&vec![rater,target]]).map_err(Self::database_error)?;
        let rows=tx.query("SELECT p.id FROM projects p CROSS JOIN actor_roles a CROSS JOIN actor_roles b WHERE a.actor_id=$1 AND b.actor_id=$2 AND a.role IN ('developer','technical_lead','pmo','manager','super_admin') AND b.role IN ('developer','technical_lead') AND NOT p.archived AND (a.all_projects OR EXISTS(SELECT 1 FROM member_role_projects x WHERE x.actor_id=a.actor_id AND x.project_id=p.id)) AND (b.all_projects OR EXISTS(SELECT 1 FROM member_role_projects x WHERE x.actor_id=b.actor_id AND x.project_id=p.id)) ORDER BY p.id FOR SHARE OF p",&[&rater,&target]).map_err(Self::database_error)?;
        Ok(rows.iter().map(|r| r.get(0)).collect())
    }
}

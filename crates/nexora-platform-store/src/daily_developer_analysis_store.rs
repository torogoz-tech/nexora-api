use crate::{
    PlatformStore, daily_developer_analysis_policy::DailyDeveloperAnalysisPolicy as Policy,
    labor_calendar_policy::LaborCalendarPolicy,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn daily_developer_analysis_directory(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::authorize(&mut tx, ctx, None)?;
        let rows=tx.query("SELECT a.id,a.name FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.active AND r.role IN ('developer','technical_lead') ORDER BY a.name,a.id",&[]).map_err(Self::database_error)?;
        let mut items = Vec::new();
        for row in rows {
            let id: String = row.get(0);
            match Policy::authorize(&mut tx, ctx, Some(&id)) {
                Ok(()) => items.push(json!({"id":id,"name":row.get::<_,String>(1)})),
                Err(PlatformError::Forbidden) => {}
                Err(error) => return Err(error),
            }
        }
        let row=tx.query_one("SELECT timezone,((clock_timestamp() AT TIME ZONE timezone)::date-1)::text FROM labor_calendar_settings WHERE singleton",&[]).map_err(Self::database_error)?;
        let result = json!({"items":items,"timezone":row.get::<_,String>(0),"latest_date":row.get::<_,String>(1)});
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn daily_developer_analysis(
        &mut self,
        ctx: &AuthContext,
        target: &str,
        date: &str,
    ) -> Result<Value, PlatformError> {
        LaborCalendarPolicy::date(&json!({"date":date}), "date")?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::authorize(&mut tx, ctx, Some(target))?;
        let future:bool=tx.query_one("SELECT $1::text::date>=(clock_timestamp() AT TIME ZONE timezone)::date FROM labor_calendar_settings WHERE singleton",&[&date]).map_err(Self::database_error)?.get(0);
        if future {
            return Err(PlatformError::Validation(
                "analysis requires a closed organization day",
            ));
        }
        let row=tx.query_opt("SELECT status,report,evidence,timezone,generated_at::text FROM daily_developer_analyses WHERE actor_id=$1 AND date=$2::text::date",&[&target,&date]).map_err(Self::database_error)?;
        if let Some(row) = &row {
            let evidence: Option<Value> = row.get(2);
            let projects = evidence
                .as_ref()
                .and_then(|e| e["project_ids"].as_array())
                .cloned()
                .unwrap_or_default();
            let repositories = evidence
                .as_ref()
                .and_then(|e| e["repository_ids"].as_array())
                .cloned()
                .unwrap_or_default();
            for repository in repositories {
                let id = repository.as_str().ok_or(PlatformError::Storage)?;
                let authorized:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM project_repositories r JOIN git_connections c ON c.id=r.connection_id JOIN actors owner ON owner.id=c.owner_actor_id WHERE r.id=$1 AND r.authorization_status='authorized' AND NOT r.archived AND c.status='active' AND owner.active)",&[&id]).map_err(Self::database_error)?.get(0);
                if !authorized {
                    return Err(PlatformError::Forbidden);
                }
            }
            for project in projects {
                let id = project.as_str().ok_or(PlatformError::Storage)?;
                let allowed:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=$1 AND (r.role='super_admin' OR r.all_projects OR EXISTS(SELECT 1 FROM member_role_projects p WHERE p.actor_id=r.actor_id AND p.project_id=$2)))",&[&ctx.actor_id(),&id]).map_err(Self::database_error)?.get(0);
                if !allowed {
                    return Err(PlatformError::Forbidden);
                }
            }
        }
        let value=row.map(|r|json!({"member_id":target,"date":date,"status":r.get::<_,String>(0),"report":r.get::<_,Option<Value>>(1),"evidence":r.get::<_,Option<Value>>(2),"timezone":r.get::<_,String>(3),"generated_at":r.get::<_,Option<String>>(4)})).unwrap_or(json!({"member_id":target,"date":date,"status":"not_available","report":null}));
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn schedule_daily_developer_analyses(&mut self) -> Result<u64, PlatformError> {
        self.client.execute("INSERT INTO daily_developer_analyses(actor_id,date,timezone) SELECT a.id,d::date,s.timezone FROM actors a JOIN actor_roles r ON r.actor_id=a.id CROSS JOIN labor_calendar_settings s CROSS JOIN LATERAL generate_series(GREATEST(coalesce((SELECT max(date)+1 FROM daily_developer_analyses x WHERE x.actor_id=a.id),(clock_timestamp() AT TIME ZONE s.timezone)::date-1),s.starts_on,(a.created_at AT TIME ZONE s.timezone)::date)::timestamp,((clock_timestamp() AT TIME ZONE s.timezone)::date-1)::timestamp,interval '1 day') d WHERE a.active AND a.kind='human' AND r.role IN ('developer','technical_lead') ON CONFLICT(actor_id,date) DO NOTHING",&[]).map_err(Self::database_error)
    }
    pub fn claim_daily_developer_analysis(&mut self) -> Result<Option<Value>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.execute("UPDATE daily_developer_analyses d SET status='failed' WHERE status='pending' AND NOT EXISTS(SELECT 1 FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.id=d.actor_id AND a.active AND r.role IN ('developer','technical_lead'))",&[]).map_err(Self::database_error)?;
        // An uncertain AI outcome is terminal: automatic lease recovery must never spend twice.
        tx.execute("UPDATE daily_developer_analyses SET status='failed',lease_token=NULL WHERE status='running' AND started_at<clock_timestamp()-interval '5 minutes'",&[]).map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT actor_id,date::text,timezone FROM daily_developer_analyses WHERE status='pending' ORDER BY date,actor_id FOR UPDATE SKIP LOCKED LIMIT 1",&[]).map_err(Self::database_error)?;
        let Some(row) = row else {
            tx.commit().map_err(Self::database_error)?;
            return Ok(None);
        };
        let actor: String = row.get(0);
        let date: String = row.get(1);
        let timezone: String = row.get(2);
        let mut evidence: Value = tx
            .query_one(
                include_str!("daily_developer_analysis_evidence.sql"),
                &[&actor, &date, &timezone],
            )
            .map_err(Self::database_error)?
            .get(0);
        evidence["availability"] = tx
            .query_one(
                "SELECT member_daily_availability($1,$2::text::date)",
                &[&actor, &date],
            )
            .map_err(Self::database_error)?
            .get(0);
        let empty = evidence["items"].as_array().is_none_or(Vec::is_empty);
        let token = crate::SecretGenerator::token("daily")?;
        let status = if empty { "no_evidence" } else { "running" };
        tx.execute("UPDATE daily_developer_analyses SET status=$3,evidence=$4,lease_token=$5,started_at=clock_timestamp(),generated_at=CASE WHEN $3='no_evidence' THEN clock_timestamp() ELSE NULL END WHERE actor_id=$1 AND date=$2::text::date AND status='pending'",&[&actor,&date,&status,&evidence,&token]).map_err(Self::database_error)?;
        let language: String = tx
            .query_one("SELECT language FROM installation_preferences LIMIT 1", &[])
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(if empty {
            None
        } else {
            Some(
                json!({"member_id":actor,"date":date,"timezone":timezone,"lease_token":token,"evidence":evidence,"language":language}),
            )
        })
    }
    pub fn finish_daily_developer_analysis(
        &mut self,
        job: &Value,
        report: Option<&Value>,
    ) -> Result<bool, PlatformError> {
        let actor = job["member_id"].as_str().ok_or(PlatformError::Storage)?;
        let date = job["date"].as_str().ok_or(PlatformError::Storage)?;
        let token = job["lease_token"].as_str().ok_or(PlatformError::Storage)?;
        let status = if report.is_some() {
            "complete"
        } else {
            "failed"
        };
        self.client.execute("UPDATE daily_developer_analyses SET status=$4,report=$5,generated_at=clock_timestamp(),lease_token=NULL WHERE actor_id=$1 AND date=$2::text::date AND status='running' AND lease_token=$3",&[&actor,&date,&token,&status,&report]).map(|n|n==1).map_err(Self::database_error)
    }
}

use crate::{PlatformStore, labor_calendar_policy::LaborCalendarPolicy as Policy};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn rating_feedback(
        &mut self,
        ctx: &AuthContext,
        year: i32,
        prepare: bool,
    ) -> Result<Value, PlatformError> {
        self.rating_feedback_for(ctx, ctx.actor_id(), year, prepare)
    }
    pub fn rating_feedback_for(
        &mut self,
        ctx: &AuthContext,
        target: &str,
        year: i32,
        prepare: bool,
    ) -> Result<Value, PlatformError> {
        if !(2000..=9998).contains(&year) {
            return Err(PlatformError::Validation("invalid ratings year"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        Self::rating_feedback_authority(&mut tx, ctx, target)?;
        if !prepare {
            let row=tx.query_opt("SELECT report,generated_at::text FROM developer_rating_feedback WHERE actor_id=$1 ORDER BY generated_at DESC NULLS LAST LIMIT 1",&[&target]).map_err(Self::database_error)?;
            let value=row.map(|r|json!({"report":r.get::<_,Option<Value>>(0),"generated_at":r.get::<_,Option<String>>(1)})).unwrap_or(json!({"report":null,"generated_at":null}));
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        // Serialize generation per recipient, including requests for different years.
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 731))",
            &[&target],
        )
        .map_err(Self::database_error)?;
        let recent:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM developer_rating_feedback WHERE actor_id=$1 AND (requested_at>clock_timestamp()-interval '2 minutes' OR generated_at>clock_timestamp()-interval '30 minutes'))",&[&target]).map_err(Self::database_error)?.get(0);
        if recent {
            return Err(PlatformError::Conflict("feedback recently requested"));
        }
        let acquired=tx.query_opt("INSERT INTO developer_rating_feedback(actor_id,year) VALUES($1,$2) ON CONFLICT(actor_id,year) DO UPDATE SET requested_at=clock_timestamp() WHERE developer_rating_feedback.requested_at<clock_timestamp()-interval '2 minutes' AND (developer_rating_feedback.generated_at IS NULL OR developer_rating_feedback.generated_at<clock_timestamp()-interval '30 minutes') RETURNING actor_id",&[&target,&year]).map_err(Self::database_error)?;
        if acquired.is_none() {
            return Err(PlatformError::Conflict("feedback recently requested"));
        }
        let report: Value = tx
            .query_one(
                include_str!("developer_rating_report.sql"),
                &[&target, &year],
            )
            .map_err(Self::database_error)?
            .get(0);
        let today:String=tx.query_one("SELECT ((clock_timestamp() AT TIME ZONE timezone)::date)::text FROM labor_calendar_settings WHERE singleton",&[]).map_err(Self::database_error)?.get(0);
        let daily = report["daily"].as_array().ok_or(PlatformError::Storage)?;
        // Only qualitative aggregate signals leave the store. Never reviewer identities,
        // private comments, individual entries or numeric ratings.
        let working: Vec<&Value> = daily
            .iter()
            .filter(|d| {
                d["excluded"] != true
                    && d["date"].as_str().is_some_and(|date| date < today.as_str())
            })
            .collect();
        let measured: Vec<f64> = working.iter().filter_map(|d| d["score"].as_f64()).collect();
        let consistency = if measured.is_empty() {
            "insufficient_data"
        } else if working.iter().filter(|d| d["push"] == true).count() * 2 >= working.len() {
            "regular"
        } else {
            "needs_consistency"
        };
        let quality = if measured.is_empty() {
            "insufficient_data"
        } else {
            let average = measured.iter().sum::<f64>() / measured.len() as f64;
            if average >= 4.0 {
                "strong"
            } else if average >= 2.5 {
                "developing"
            } else {
                "needs_support"
            }
        };
        let language: String = tx
            .query_one("SELECT language FROM installation_preferences LIMIT 1", &[])
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"consistency":consistency,"quality":quality,"language":language}))
    }
    pub fn finish_rating_feedback(
        &mut self,
        ctx: &AuthContext,
        year: i32,
        report: &Value,
    ) -> Result<Value, PlatformError> {
        self.finish_rating_feedback_for(ctx, ctx.actor_id(), year, report)
    }
    pub fn finish_rating_feedback_for(
        &mut self,
        ctx: &AuthContext,
        target: &str,
        year: i32,
        report: &Value,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        Self::rating_feedback_authority(&mut tx, ctx, target)?;
        let row=tx.query_one("UPDATE developer_rating_feedback SET report=$3,generated_at=clock_timestamp() WHERE actor_id=$1 AND year=$2 RETURNING generated_at::text",&[&target,&year,&report]).map_err(Self::database_error)?;
        // Keep only the member's current feedback after successful replacement.
        tx.execute(
            "DELETE FROM developer_rating_feedback WHERE actor_id=$1 AND year<>$2",
            &[&target, &year],
        )
        .map_err(Self::database_error)?;
        let value = json!({"report":report,"generated_at":row.get::<_,String>(0)});
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}

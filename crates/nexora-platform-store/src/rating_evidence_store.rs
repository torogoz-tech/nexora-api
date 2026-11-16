use crate::{
    PlatformStore, SecretGenerator, labor_calendar_policy::LaborCalendarPolicy,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn upload_rating_evidence(
        &mut self,
        ctx: &AuthContext,
        target: &str,
        project: &str,
        name: &str,
        image: bool,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        if name.trim().is_empty()
            || name.len() > 240
            || name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
            || bytes.is_empty()
            || bytes.len() > 20971520
            || (image && !bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        {
            return Err(PlatformError::Validation("invalid rating evidence"));
        }
        let project = if project.is_empty() {
            None
        } else {
            Some(project)
        };
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        Self::rating_target(&mut tx, target)?;
        if ctx.actor_id() == target {
            return Err(PlatformError::Forbidden);
        }
        Self::rating_general_authority(&mut tx, ctx.actor_id())?;
        if let Some(project) = project {
            if !Self::rating_projects(&mut tx, ctx.actor_id(), target)?
                .iter()
                .any(|p| p == project)
            {
                return Err(PlatformError::Forbidden);
            }
            PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, Some(project))?;
        }
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("rating-evidence:{}", ctx.actor_id())],
        )
        .map_err(Self::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "rating.evidence.upload",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        tx.execute("DELETE FROM rating_evidence WHERE uploaded_by=$1 AND rating_id IS NULL AND created_at<clock_timestamp()-interval '24 hours'",&[&ctx.actor_id()]).map_err(Self::database_error)?;
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM rating_evidence WHERE uploaded_by=$1 AND rating_id IS NULL",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .get(0);
        if count >= 32 {
            return Err(PlatformError::Validation("too many pending evidence files"));
        }
        let id = SecretGenerator::id("rev")?;
        let sha = SecretGenerator::checksum(bytes);
        tx.execute("INSERT INTO rating_evidence(id,target_id,project_id,uploaded_by,name,image,payload,sha256) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&id,&target,&project,&ctx.actor_id(),&name,&image,&bytes,&sha]).map_err(Self::database_error)?;
        let result = json!({"id":id,"name":name,"image":image,"size_bytes":bytes.len(),"url":format!("/api/v1/rating-evidence/{id}/content")});
        ReceiptWriter::save(&mut tx, request.as_ref(), &result)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn rating_evidence_content(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(bool, Vec<u8>), PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        let row=tx.query_opt("SELECT target_id,project_id,uploaded_by,rating_id,image,payload,sha256,created_at>clock_timestamp()-interval '24 hours' FROM rating_evidence WHERE id=$1",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        let target: String = row.get(0);
        let project: Option<String> = row.get(1);
        let owner: String = row.get(2);
        let attached: Option<String> = row.get(3);
        if ctx.actor_id() == target {
            return Err(PlatformError::Forbidden);
        }
        Self::rating_target(&mut tx, &target)?;
        let shared = Self::rating_projects(&mut tx, ctx.actor_id(), &target)?;
        let admin = Self::rating_admin(&mut tx, ctx.actor_id())?;
        let lead = Self::rating_lead(&mut tx, ctx.actor_id())?;
        if attached.is_none() {
            if owner != ctx.actor_id()
                || !row.get::<_, bool>(7)
                || project.as_ref().is_some_and(|p| !shared.contains(p))
            {
                return Err(PlatformError::NotFound);
            }
        } else if !admin
            && !((lead || owner == ctx.actor_id())
                && project.as_ref().is_none_or(|p| shared.contains(p)))
        {
            return Err(PlatformError::Forbidden);
        }
        if !admin {
            Self::rating_general_authority(&mut tx, ctx.actor_id())?;
            if let Some(project) = project.as_deref() {
                PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, Some(project))?;
            }
        }
        let bytes: Vec<u8> = row.get(5);
        if SecretGenerator::checksum(&bytes) != row.get::<_, String>(6) {
            return Err(PlatformError::Storage);
        }
        let image: bool = row.get(4);
        tx.commit().map_err(Self::database_error)?;
        Ok((image, bytes))
    }
}

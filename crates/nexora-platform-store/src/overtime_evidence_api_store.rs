use crate::{
    PlatformStore, audit_writer::AuditWriter, labor_calendar_policy::LaborCalendarPolicy,
    overtime_evidence_store::OvertimeEvidenceStore, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn overtime_planned_evidence(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        LaborCalendarPolicy::member(&mut tx, ctx.actor_id())?;
        let mut tasks = OvertimeEvidenceStore::planned_tasks(&mut tx, ctx)?;
        let has_more = tasks.len() > 500;
        tasks.truncate(500);
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"tasks":tasks,"commits":[],"has_more":has_more}))
    }
    pub fn overtime_evidence_candidates(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        LaborCalendarPolicy::member(&mut tx, ctx.actor_id())?;
        let value = OvertimeEvidenceStore::candidates(&mut tx, ctx, id)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn append_overtime_evidence(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let object = input
            .as_object()
            .ok_or(PlatformError::Validation("invalid overtime evidence"))?;
        if object
            .keys()
            .any(|k| !matches!(k.as_str(), "task_ids" | "source_event_ids"))
        {
            return Err(PlatformError::Validation("unknown overtime evidence field"));
        }
        let tasks = OvertimeEvidenceStore::ids(input.get("task_ids"), false)?;
        let events = OvertimeEvidenceStore::ids(input.get("source_event_ids"), false)?;
        if tasks.is_empty() && events.is_empty() {
            return Err(PlatformError::Validation("evidence is required"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        LaborCalendarPolicy::member(&mut tx, ctx.actor_id())?;
        tx.query_opt("SELECT id FROM labor_overtime_requests WHERE id=$1 AND actor_id=$2 AND status IN ('pending','approved') FOR UPDATE",&[&id,&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Forbidden)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "labor.overtime.evidence.append",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        if !tasks.is_empty() {
            OvertimeEvidenceStore::bind_tasks(&mut tx, ctx, id, &tasks)?;
        }
        if !events.is_empty() {
            let commits = OvertimeEvidenceStore::verified_commits(&mut tx, ctx, id, Some(&events))?;
            if commits.len() != events.len() {
                return Err(PlatformError::Validation(
                    "commit evidence must be a verified own push receipt inside the overtime interval and a linked task project",
                ));
            }
            for event in &events {
                tx.execute("INSERT INTO overtime_commit_links(request_id,source_event_id,added_by) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&id,&event,&ctx.actor_id()]).map_err(Self::database_error)?;
            }
        }
        let value = OvertimeEvidenceStore::projection(&mut tx, ctx, id)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "labor.overtime.evidence.append",
            Some(id),
            json!({"task_ids":tasks,"source_event_ids":events}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}

use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    feature_governance_policy::FeatureGovernancePolicy as Policy,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_definition_store::TaskDefinitionStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn feature_catalog(
        &mut self,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::read(&mut tx, ctx, project)?;
        let can_decide = match Policy::human(&mut tx, ctx, project) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(e) => return Err(e),
        };
        let manager = tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('manager','super_admin') FOR SHARE", &[&ctx.actor_id()]).map_err(Self::database_error)?.is_some();
        let can_complete = if manager {
            match PlatformAuthority::require(
                &mut tx,
                ctx,
                Capability::DocumentsWrite,
                Some(project),
            ) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(error) => return Err(error),
            }
        } else {
            false
        };
        let revision: i64 = tx
            .query_opt(
                "SELECT revision FROM feature_catalogs WHERE project_id=$1",
                &[&project],
            )
            .map_err(Self::database_error)?
            .map(|r| r.get(0))
            .unwrap_or(0);
        let items:Vec<Value>=tx.query("SELECT to_jsonb(f)||jsonb_build_object('requirement_ids',COALESCE((SELECT jsonb_agg(r.requirement_id ORDER BY r.requirement_id) FROM requirement_features r WHERE r.feature_id=f.id),'[]'::jsonb),'task_ids',COALESCE((SELECT jsonb_agg(linked.task_id ORDER BY linked.task_id) FROM (SELECT t.task_id FROM task_feature_links t WHERE t.feature_id=f.id UNION SELECT a.task_id FROM task_feature_associations a WHERE a.feature_id=f.id) linked),'[]'::jsonb)) FROM project_features f WHERE project_id=$1 ORDER BY name,id LIMIT 1000",&[&project]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        let proposals:Vec<Value>=tx.query("SELECT to_jsonb(p) FROM feature_proposals p WHERE project_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100",&[&project]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        let links:Vec<Value>=tx.query("SELECT to_jsonb(r) FROM requirement_features r WHERE project_id=$1 ORDER BY requirement_id,feature_id LIMIT 4000",&[&project]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(
            json!({"items":items,"proposals":proposals,"links":links,"catalog_revision":revision,"can_decide":can_decide,"can_complete":can_complete}),
        )
    }
    pub fn propose_feature_map(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &FeatureProposalCreate,
    ) -> Result<Value, PlatformError> {
        let reason = DomainText::required(&input.reason, 4000)?;
        if input.tasks.len() > 50
            || input
                .tasks
                .iter()
                .any(|t| t.feature_index >= input.links.len())
            || input.expected_catalog_revision < 0
        {
            return Err(PlatformError::Validation(
                "invalid feature map tasks or catalog revision",
            ));
        }
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::read(&mut tx, ctx, project)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::DocumentsWrite, Some(project))?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "feature_map.propose",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let revision = Policy::catalog(&mut tx, project)?;
        if revision != input.expected_catalog_revision {
            return Err(PlatformError::Conflict(
                "feature catalog changed; inspect existing features first",
            ));
        }
        let title = Policy::requirement(
            &mut tx,
            project,
            &input.requirement_id,
            &input.requirement_version_id,
        )?;
        Policy::links(&mut tx, project, &input.links)?;
        for task in &input.tasks {
            DomainText::required(&task.definition.title, 256)?;
        }
        let id = SecretGenerator::id("fmp")?;
        let links = json!(input.links);
        let tasks = json!(input.tasks);
        let value:Value=tx.query_one("INSERT INTO feature_proposals AS p(id,project_id,requirement_id,requirement_version_id,requirement_title,catalog_revision,reason,links,tasks,proposed_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING to_jsonb(p)",&[&id,&project,&input.requirement_id,&input.requirement_version_id,&title,&revision,&reason,&links,&tasks,&ctx.actor_id()]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "feature_map.proposed",
            Some(&id),
            json!({"project_id":project,"requirement_id":input.requirement_id}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn decide_feature_map(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &FeatureProposalDecision,
    ) -> Result<Value, PlatformError> {
        let reason = DomainText::optional(&input.reason, 4000)?;
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::read(&mut tx, ctx, project)?;
        Policy::human(&mut tx, ctx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "feature_map.decide",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        // One project catalog lock serializes decisions, duplicate feature detection and map changes.
        Policy::catalog(&mut tx, project)?;
        let proposal:Value=tx.query_opt("SELECT to_jsonb(p) FROM feature_proposals p WHERE id=$1 AND project_id=$2 FOR UPDATE",&[&id,&project]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        if proposal["status"] != "pending"
            || proposal["revision"].as_i64() != Some(input.expected_revision)
        {
            return Err(PlatformError::Conflict(
                "feature proposal already decided or revision changed",
            ));
        }
        let requirement = proposal["requirement_id"]
            .as_str()
            .ok_or(PlatformError::Storage)?;
        let mut result = Value::Null;
        if input.decision == ApprovalDecision::Approved {
            // Revalidate the exact requirement and feature dependencies under the catalog lock.
            // An unrelated approval must not invalidate every pending proposal in the project.
            Policy::requirement(
                &mut tx,
                project,
                requirement,
                proposal["requirement_version_id"]
                    .as_str()
                    .ok_or(PlatformError::Storage)?,
            )?;
            let links: Vec<FeatureLinkProposal> = Self::decode(proposal["links"].clone())?;
            let tasks: Vec<FeatureTaskProposal> = Self::decode(proposal["tasks"].clone())?;
            Policy::links(&mut tx, project, &links)?;
            let mut feature_ids = Vec::new();
            for link in links {
                let feature = if let Some(existing) = link.feature_id {
                    existing
                } else {
                    let new_id = SecretGenerator::id("fea")?;
                    let name = DomainText::name(link.name.as_deref().unwrap_or(""))?;
                    tx.execute("INSERT INTO project_features(id,project_id,name,description,created_by) VALUES($1,$2,$3,$4,$5)",&[&new_id,&project,&name,&link.description.unwrap_or_default(),&ctx.actor_id()]).map_err(Self::database_error)?;
                    new_id
                };
                tx.execute("INSERT INTO requirement_features(project_id,requirement_id,feature_id,impact,proposal_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT(project_id,requirement_id,feature_id) DO UPDATE SET impact=EXCLUDED.impact,proposal_id=EXCLUDED.proposal_id",&[&project,&requirement,&feature,&link.impact,&id]).map_err(Self::database_error)?;
                feature_ids.push(feature);
            }
            let mut task_ids = Vec::new();
            if !tasks.is_empty() {
                PlatformAuthority::require(&mut tx, ctx, Capability::TasksWrite, Some(project))?;
            }
            for proposed in tasks {
                let feature = feature_ids
                    .get(proposed.feature_index)
                    .ok_or(PlatformError::Storage)?;
                let mut definition = proposed.definition;
                if definition.primary_requirement_id.is_some()
                    || definition
                        .primary_feature_id
                        .as_ref()
                        .is_some_and(|id| id != feature)
                {
                    return Err(PlatformError::Validation(
                        "feature proposal tasks must use their approved primary feature",
                    ));
                }
                definition.primary_feature_id = Some(feature.clone());
                let task =
                    TaskDefinitionStore::insert(&mut tx, ctx, project, &definition, None, true)?;
                tx.execute("INSERT INTO task_feature_links(project_id,task_id,feature_id,requirement_id) VALUES($1,$2,$3,$4)",&[&project,&task.id,&feature,&requirement]).map_err(Self::database_error)?;
                task_ids.push(task.id);
            }
            tx.execute(
                "UPDATE feature_catalogs SET revision=revision+1 WHERE project_id=$1",
                &[&project],
            )
            .map_err(Self::database_error)?;
            result = json!({"feature_ids":feature_ids,"task_ids":task_ids});
        }
        let status = if input.decision == ApprovalDecision::Approved {
            "approved"
        } else {
            "rejected"
        };
        let value:Value=tx.query_one("UPDATE feature_proposals p SET status=$1,decided_by=$2,decision_reason=$3,decided_at=clock_timestamp(),revision=revision+1,result=$4 WHERE id=$5 RETURNING to_jsonb(p)",&[&status,&ctx.actor_id(),&reason,&result,&id]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "feature_map.decided",
            Some(id),
            json!({"project_id":project,"decision":status,"result":result}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}

use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::json;
impl PlatformStore {
    pub fn member_role(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<MemberRoleAssignment, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::MembersRead, None).or_else(
            |error| match error {
                PlatformError::Forbidden => {
                    PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)
                }
                error => Err(error),
            },
        )?;
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND kind='human'",
            &[&actor],
        )
        .map_err(Self::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let value = Self::read_member_role(&mut tx, actor)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn change_member_role(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &MemberRoleChange,
    ) -> Result<MemberRoleAssignment, PlatformError> {
        input.validate()?;
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let admin =
            match PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(error) => return Err(error),
            };
        if !admin {
            tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role='technical_lead' FOR SHARE",&[&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Forbidden)?;
        }
        // Serialize role writes and legacy grant writes on their common actor row.
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND kind='human' AND active FOR UPDATE",
            &[&actor],
        )
        .map_err(Self::database_error)?
        .ok_or(PlatformError::NotFound)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "member.role.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let old = Self::read_member_role(&mut tx, actor)?;
        if !admin {
            if old.role != Some(input.role) {
                return Err(PlatformError::Forbidden);
            }
            if input.all_projects || old.all_projects {
                PlatformAuthority::require(&mut tx, ctx, Capability::MemberProjectsManage, None)?;
            } else {
                let before: std::collections::BTreeSet<_> = old.project_ids.iter().collect();
                let after: std::collections::BTreeSet<_> = input.project_ids.iter().collect();
                if before == after {
                    return Err(PlatformError::Conflict("member projects unchanged"));
                }
                for project in before.symmetric_difference(&after) {
                    PlatformAuthority::require(
                        &mut tx,
                        ctx,
                        Capability::MemberProjectsManage,
                        Some(project.as_str()),
                    )?;
                }
            }
        }
        if old.role == Some(MemberRole::SuperAdmin) {
            return Err(PlatformError::Forbidden);
        }
        if old.revision != input.expected_revision {
            return Err(PlatformError::Conflict("member role changed"));
        }
        if input.role == MemberRole::Developer
            && tx
                .query_one(
                    "SELECT count(*)>1 FROM developer_workflows WHERE actor_id=$1",
                    &[&actor],
                )
                .map_err(Self::database_error)?
                .get::<_, bool>(0)
        {
            return Err(PlatformError::Conflict(
                "keep only one workflow before changing role to developer",
            ));
        }
        for project in &input.project_ids {
            PlatformAuthority::project(&mut tx, project)?;
        }
        tx.execute("INSERT INTO actor_roles(actor_id,role,all_projects,revision) VALUES($1,$2,$3,1) ON CONFLICT(actor_id) DO UPDATE SET role=EXCLUDED.role,all_projects=EXCLUDED.all_projects,revision=actor_roles.revision+1", &[&actor,&input.role.as_str(),&input.all_projects]).map_err(Self::database_error)?;
        tx.execute(
            "DELETE FROM member_role_projects WHERE actor_id=$1",
            &[&actor],
        )
        .map_err(Self::database_error)?;
        tx.execute("DELETE FROM actor_grants WHERE actor_id=$1", &[&actor])
            .map_err(Self::database_error)?;
        for project in &input.project_ids {
            tx.execute(
                "INSERT INTO member_role_projects(actor_id,project_id) VALUES($1,$2)",
                &[&actor, &project],
            )
            .map_err(Self::database_error)?;
        }
        let scopes: Vec<Option<&str>> = if input.all_projects {
            vec![None]
        } else {
            input
                .project_ids
                .iter()
                .map(|id| Some(id.as_str()))
                .collect()
        };
        let configuration = input.role.configuration_capabilities();
        for capability in input.role.capabilities() {
            let capability_scopes = if configuration.contains(&capability)
                || matches!(
                    capability,
                    Capability::RequestsRead
                        | Capability::RequestsWrite
                        | Capability::OrganizationKnowledgeRead
                        | Capability::OrganizationKnowledgeWrite
                        | Capability::KnowledgeContribute
                        | Capability::AnalysisProfilesRead
                        | Capability::AnalysisProfilesContribute
                ) {
                vec![None]
            } else {
                scopes.clone()
            };
            for project in &capability_scopes {
                tx.execute(
                    "INSERT INTO actor_grants(actor_id,capability,project_id) VALUES($1,$2,$3)",
                    &[&actor, &capability.as_str(), project],
                )
                .map_err(Self::database_error)?;
            }
        }
        let value = Self::read_member_role(&mut tx, actor)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "member.role.change",
            Some(actor),
            json!(value),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &json!(value))?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    fn read_member_role(
        tx: &mut postgres::Transaction<'_>,
        actor: &str,
    ) -> Result<MemberRoleAssignment, PlatformError> {
        let row = tx.query_one("SELECT jsonb_build_object('role',(SELECT role FROM actor_roles WHERE actor_id=$1),'all_projects',COALESCE((SELECT all_projects FROM actor_roles WHERE actor_id=$1),false),'project_ids',COALESCE((SELECT jsonb_agg(project_id ORDER BY project_id) FROM member_role_projects WHERE actor_id=$1),'[]'::jsonb),'revision',COALESCE((SELECT revision FROM actor_roles WHERE actor_id=$1),0))", &[&actor]).map_err(Self::database_error)?;
        Self::decode(row.get(0))
    }
}

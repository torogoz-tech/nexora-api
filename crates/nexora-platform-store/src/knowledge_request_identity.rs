use nexora_platform::PlatformError;
use nexora_runtime::KnowledgeCommand;
use sha2::{Digest, Sha256};

pub(crate) struct KnowledgeRequestIdentity;
impl KnowledgeRequestIdentity {
    pub fn operation(command: &KnowledgeCommand) -> Result<&'static str, PlatformError> {
        match command {
            KnowledgeCommand::Contribute { .. } => Ok("knowledge.contribute"),
            KnowledgeCommand::Revise { .. } => Ok("knowledge.revise"),
            KnowledgeCommand::Review { .. } => Ok("knowledge.review"),
            _ => Err(PlatformError::Validation("knowledge mutation required")),
        }
    }
    pub fn internal(
        actor: &str,
        project: &str,
        repository: &str,
        operation: &str,
        request: &str,
    ) -> Result<String, PlatformError> {
        let bytes = serde_json::to_vec(&(actor, project, repository, operation, request))
            .map_err(|_| PlatformError::Storage)?;
        Ok(format!("api:{:x}", Sha256::digest(bytes)))
    }
    pub fn command(command: &KnowledgeCommand, request: String) -> KnowledgeCommand {
        let mut command = command.clone();
        match &mut command {
            KnowledgeCommand::Contribute { request_id, .. }
            | KnowledgeCommand::Revise { request_id, .. }
            | KnowledgeCommand::Review { request_id, .. } => *request_id = request,
            _ => {}
        }
        command
    }
}

use crate::PlatformStore;
use nexora_platform::*;
use postgres::Transaction;
use serde_json::Value;
pub(crate) struct TaskTestedRevision;
impl TaskTestedRevision {
    pub fn validate(
        tx: &mut Transaction<'_>,
        parent: &WorkTask,
        repository: &str,
        sha: &str,
    ) -> Result<(), PlatformError> {
        let delivery=tx.query_opt("SELECT details FROM task_history WHERE task_id=$1 AND operation='task.workflow_delivered' AND details->'after'->>'workflow_version_id'=$2 ORDER BY sequence DESC LIMIT 1",&[&parent.id,&parent.workflow_version_id]).map_err(PlatformStore::database_error)?;
        if let Some(row) = delivery {
            let details: Value = row.get(0);
            let proven = details["evidence"].as_array().is_some_and(|steps| {
                steps.iter().any(|step| {
                    step["kind"] == "verify_deployment"
                        && step["result"]["deployments"]
                            .as_array()
                            .is_some_and(|deployments| {
                                deployments.iter().any(|deployment| {
                                    deployment["repository_id"] == repository
                                        && deployment["head_sha"] == sha
                                        && deployment["merge"]["merge_sha"] == sha
                                        && deployment["status"] == "success"
                                        && deployment["provenance"] == "provider_verified"
                                })
                            })
                })
            });
            if proven {
                return Ok(());
            }
            return Err(PlatformError::Conflict(
                "corrective subtask must use the exact repository revision deployed for the parent workflow stage",
            ));
        }
        let proven:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_repository_branches WHERE task_id=$1 AND repository_id=$2 AND head_sha=$3 AND evidence->>'provenance'='provider_verified')",&[&parent.id,&repository,&sha]).map_err(PlatformStore::database_error)?.get(0);
        if !proven {
            return Err(PlatformError::Conflict(
                "verify the parent task's tested repository revision before creating corrective work",
            ));
        }
        Ok(())
    }
}

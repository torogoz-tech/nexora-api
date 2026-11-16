use crate::*;
use sha2::{Digest, Sha256};
pub struct GovernanceReporter;
impl GovernanceReporter {
    pub fn json(result: &EvaluationResult) -> Result<String, GovernanceError> {
        Self::verify(result)?;
        Ok(serde_json::to_string_pretty(result)?)
    }
    pub fn text(result: &EvaluationResult) -> String {
        let mut text = format!(
            "{}: evaluation {}\ntenant={} repository={} snapshot={}\n",
            format!("{:?}", result.status).to_uppercase(),
            result.id,
            result.tenant_id.0.escape_debug(),
            result.repository_id.0.escape_debug(),
            result.snapshot_id.0.escape_debug()
        );
        for finding in &result.findings {
            text.push_str(&format!(
                "{} {}@{} node={}\n",
                format!("{:?}", finding.status).to_uppercase(),
                finding.policy_id,
                finding.policy_version.0,
                finding
                    .node_id
                    .as_ref()
                    .map_or("<selection>", |n| n.0.as_str())
                    .escape_debug()
            ));
            for evidence in &finding.evidence {
                let location = match (&evidence.path, &evidence.span) {
                    (Some(path), Some(span)) => format!(
                        "{}:{}:{} bytes={}..{}",
                        path.escape_debug(),
                        span.start.line,
                        span.start.column,
                        span.start.offset,
                        span.end.offset
                    ),
                    _ => "<location unavailable: insufficient evidence>".into(),
                };
                text.push_str(&format!(
                    "  {} {}: {} [exhaustive={}]\n",
                    location,
                    evidence.predicate,
                    evidence.message.escape_debug(),
                    evidence.exhaustive
                ));
            }
            for diagnostic in &finding.diagnostics {
                text.push_str(&format!(
                    "  analysis {} {}: {}\n",
                    diagnostic.stage.escape_debug(),
                    diagnostic
                        .path
                        .as_deref()
                        .unwrap_or("<repository>")
                        .escape_debug(),
                    diagnostic.message.escape_debug()
                ));
            }
        }
        text
    }
    pub fn exit_code(result: &EvaluationResult, unknown_override: Option<bool>) -> u8 {
        if result.findings.iter().any(|f| {
            f.status == EvaluationStatus::Fail
                || (f.status == EvaluationStatus::Unknown
                    && unknown_override.unwrap_or(f.on_unknown == UnknownBehavior::Fail))
        }) {
            1
        } else {
            0
        }
    }
    pub fn verify(result: &EvaluationResult) -> Result<(), GovernanceError> {
        let mut content = result.clone();
        content.id.clear();
        let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&content)?));
        if result.id != hash {
            return Err(GovernanceError::Configuration(
                "evaluation content does not match its identity".into(),
            ));
        }
        Ok(())
    }
}

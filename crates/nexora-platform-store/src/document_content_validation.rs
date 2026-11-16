use nexora_platform::*;
use serde_json::Value;

pub(crate) struct DocumentContentValidation;
impl DocumentContentValidation {
    pub fn validate(content: &DocumentContent) -> Result<Value, PlatformError> {
        let value = serde_json::to_value(content).map_err(|_| PlatformError::Storage)?;
        Self::bounded(&value)?;
        if serde_json::to_vec(&value)
            .map_err(|_| PlatformError::Storage)?
            .len()
            > 65536
        {
            return Err(PlatformError::Validation("document content exceeds limit"));
        }
        match content {
            DocumentContent::Requirement(body) => {
                Self::required(&[&body.problem, &body.objective, &body.scope, &body.source])?;
                if body.acceptance_criteria.is_empty() || body.business_actors.is_empty() {
                    return Err(PlatformError::Validation(
                        "requirement needs actors and acceptance criteria",
                    ));
                }
            }
            DocumentContent::BusinessRule(body) => {
                Self::required(&[&body.statement, &body.source])?;
                if body.conditions.is_empty() {
                    return Err(PlatformError::Validation(
                        "business rule needs explicit conditions",
                    ));
                }
            }
            DocumentContent::Decision(body) => {
                Self::required(&[&body.context, &body.decision, &body.source])?;
            }
            DocumentContent::BestPractice(body) => {
                Self::required(&[&body.guideline, &body.rationale, &body.source])?;
            }
            DocumentContent::TechnicalDebt(body) => {
                Self::required(&[&body.problem, &body.impact, &body.remediation, &body.source])?;
            }
            DocumentContent::Document(body) => {
                Self::required(&[&body.text, &body.source])?;
                if let Some(architecture) = &body.architecture {
                    Self::required(&[&architecture.style])?;
                }
            }
        }
        Ok(value)
    }
    fn required(fields: &[&String]) -> Result<(), PlatformError> {
        for field in fields {
            DomainText::required(field, 16000)?;
        }
        Ok(())
    }
    fn bounded(value: &Value) -> Result<(), PlatformError> {
        match value {
            Value::String(text) => {
                DomainText::required(text, 16000)?;
            }
            Value::Array(items) => {
                if items.len() > 128 {
                    return Err(PlatformError::Validation("too many document statements"));
                }
                for item in items {
                    Self::bounded(item)?;
                }
            }
            Value::Object(fields) => {
                for item in fields.values() {
                    Self::bounded(item)?;
                }
            }
            Value::Null => {}
            _ => {
                return Err(PlatformError::Validation(
                    "unexpected structured document value",
                ));
            }
        }
        Ok(())
    }
    pub fn code(value: &str) -> Result<String, PlatformError> {
        let code = value.trim().to_ascii_lowercase();
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(PlatformError::Validation("invalid document code"));
        }
        Ok(code)
    }
}

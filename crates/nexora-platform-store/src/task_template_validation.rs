use crate::task_policy_store::TaskPolicyStore;
use nexora_platform::*;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(crate) struct TaskTemplateValidation;
impl TaskTemplateValidation {
    pub fn fields(input: &TaskTemplateFields) -> Result<TaskTemplateFields, PlatformError> {
        let mut fields = input.clone();
        fields.title = input
            .title
            .as_deref()
            .map(|value| DomainText::required(value, 256))
            .transpose()?;
        fields.context = TaskPolicyStore::context(&input.context)?;
        fields.type_code = DomainText::required(&input.type_code, 64)?.to_ascii_lowercase();
        TaskPolicyStore::fields(&fields.fields)?;
        if serde_json::to_vec(&fields)
            .map_err(|_| PlatformError::Storage)?
            .len()
            > 32768
        {
            return Err(PlatformError::Validation(
                "template definition exceeds limit",
            ));
        }
        Ok(fields)
    }
    pub fn timestamp(value: &str) -> Result<OffsetDateTime, PlatformError> {
        OffsetDateTime::parse(value, &Rfc3339)
            .map_err(|_| PlatformError::Validation("template timestamps must be RFC3339"))
    }
}

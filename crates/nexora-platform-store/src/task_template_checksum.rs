use crate::{SecretGenerator, task_template_validation::TaskTemplateValidation};
use nexora_platform::{PlatformError, TaskTemplateVersion};
use serde_json::{Value, json};

pub(crate) struct TaskTemplateChecksum;
impl TaskTemplateChecksum {
    pub fn calculate(version: &TaskTemplateVersion) -> Result<String, PlatformError> {
        let from = TaskTemplateValidation::timestamp(&version.valid_from)?
            .unix_timestamp_nanos()
            .to_string();
        let until = version
            .valid_until
            .as_deref()
            .map(TaskTemplateValidation::timestamp)
            .transpose()?
            .map(|value| value.unix_timestamp_nanos().to_string());
        let content = json!({"project_id":version.project_id,"template_id":version.template_id,"number":version.number,"previous_version_id":version.previous_version_id,"code":version.code,"name":version.name,"definition":version.definition,"valid_from":from,"valid_until":until});
        let bytes =
            serde_json::to_vec(&Self::canonical(content)).map_err(|_| PlatformError::Storage)?;
        Ok(SecretGenerator::checksum(&bytes))
    }
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut fields = map.into_iter().collect::<Vec<_>>();
                fields.sort_unstable_by(|left, right| left.0.cmp(&right.0));
                Value::Object(
                    fields
                        .into_iter()
                        .map(|(key, value)| (key, Self::canonical(value)))
                        .collect(),
                )
            }
            Value::Array(items) => Value::Array(items.into_iter().map(Self::canonical).collect()),
            other => other,
        }
    }
}

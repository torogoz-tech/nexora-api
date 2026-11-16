use crate::{OpenAiClient, ReasoningError};
use serde_json::{Value, json};
use std::time::Duration;
pub struct DeveloperFeedback;
impl DeveloperFeedback {
    pub fn run(
        client: &OpenAiClient,
        input: &Value,
        effort: &str,
    ) -> Result<Value, ReasoningError> {
        let string = json!({"type":"string","minLength":1,"maxLength":2000});
        let list = json!({"type":"array","maxItems":4,"items":string});
        let schema = json!({"type":"object","additionalProperties":false,"required":["summary","strengths","improvements","actions"],"properties":{"summary":string,"strengths":list,"improvements":list,"actions":list}});
        let response=client.send(&json!({"model":client.model,"store":false,"reasoning":{"effort":effort},"max_output_tokens":800,"tools":[],"input":serde_json::to_string(input).map_err(|_|ReasoningError::InvalidInput)?,"instructions":"Write concise, concrete professional development feedback in the supplied language (es or en), addressing the person directly. Only the qualitative aggregate signals supplied are evidence. Do not infer personality, intent, health, protected characteristics or employment suitability. Never identify reviewers, quote evaluations, disclose or invent numeric scores, rankings, counts or percentages. No numbers. Explain insufficient data candidly; do not treat missing data as poor performance. Offer practical actions without inventing accomplishments. This is coaching, never an employment decision. Use at most two short sentences in summary. Include only evidence-supported observations. Do not repeat the summary in the lists. If evidence is insufficient, say so in one short sentence and leave all lists empty. Otherwise include at most one practical action and avoid generic coaching or filler. Return only the required schema.","text":{"format":{"type":"json_schema","name":"developer_feedback","strict":true,"schema":schema}}}),Duration::from_secs(60))?;
        if response["status"] != "completed" {
            return Err(ReasoningError::InvalidResponse);
        }
        let texts: Vec<&str> = response["output"]
            .as_array()
            .ok_or(ReasoningError::InvalidResponse)?
            .iter()
            .filter(|v| v["type"] == "message")
            .flat_map(|v| v["content"].as_array().into_iter().flatten())
            .filter(|v| v["type"] == "output_text")
            .filter_map(|v| v["text"].as_str())
            .collect();
        if texts.len() != 1 {
            return Err(ReasoningError::InvalidResponse);
        }
        let value: Value =
            serde_json::from_str(texts[0]).map_err(|_| ReasoningError::InvalidResponse)?;
        Self::validate(&value)?;
        Ok(value)
    }
    pub fn validate(value: &Value) -> Result<(), ReasoningError> {
        let valid =
            |s: &str| !s.trim().is_empty() && s.len() <= 8000 && !s.chars().any(char::is_numeric);
        if value.as_object().is_none_or(|v| v.len() != 4)
            || value["summary"].as_str().is_none_or(|s| !valid(s))
        {
            return Err(ReasoningError::InvalidResponse);
        }
        for key in ["strengths", "improvements", "actions"] {
            if value[key].as_array().is_none_or(|a| {
                a.len() > 4 || a.iter().any(|v| v.as_str().is_none_or(|s| !valid(s)))
            }) {
                return Err(ReasoningError::InvalidResponse);
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_numeric_scores_and_extra_private_fields() {
        let safe = json!({"summary":"Buen progreso, continúa fortaleciendo la consistencia.","strengths":[],"improvements":[],"actions":["Documenta tus avances."]});
        assert!(DeveloperFeedback::validate(&safe).is_ok());
        let mut invalid = safe.clone();
        invalid["summary"] = json!("Tu nota es 4 de 5");
        assert!(DeveloperFeedback::validate(&invalid).is_err());
        let mut invalid = safe;
        invalid["reviewer"] = json!("Private reviewer");
        assert!(DeveloperFeedback::validate(&invalid).is_err());
    }
}

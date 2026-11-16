use nexora_platform::{DocumentExport, PlatformError, ProjectDocumentVersion};
use serde_json::Value;

pub(crate) struct DocumentExportRenderer;
impl DocumentExportRenderer {
    pub fn render(
        version: &ProjectDocumentVersion,
        format: &str,
    ) -> Result<DocumentExport, PlatformError> {
        let mut value = serde_json::to_value(version).map_err(|_| PlatformError::Storage)?;
        // Export immutable authored content, without mutable lifecycle fields.
        let fields = value.as_object_mut().ok_or(PlatformError::Storage)?;
        fields.remove("state_id");
        fields.remove("state_revision");
        fields.remove("needs_review");
        let (media_type, content) = match format {
            "json" => (
                "application/json",
                serde_json::to_string_pretty(&value).map_err(|_| PlatformError::Storage)?,
            ),
            "markdown" => {
                let mut text = format!(
                    "# {}\n\nVersion: {}\n\nChecksum: `{}`\n\nAuthor: {}\n\nOrigin: {}\n\n",
                    Self::escape(&version.title),
                    version.number,
                    version.checksum,
                    Self::escape(&version.created_by),
                    value["origin"].as_str().ok_or(PlatformError::Storage)?
                );
                Self::section(&mut text, &version.content["body"], 2);
                if let Some(manifest) = version.context_manifest_id.as_deref() {
                    text.push_str(&format!(
                        "\n## Source context\n\nManifest: `{manifest}`\n\n"
                    ));
                    for evidence in &version.evidence {
                        text.push_str(&format!(
                            "Repository: `{}`\n\nSnapshot: `{}`\n\n",
                            evidence.repository_id, evidence.snapshot_id
                        ));
                        if let Some(commit) = &evidence.pinned_commit {
                            text.push_str(&format!("Commit: `{commit}`\n\n"));
                        } else if let Some(origin) = &evidence.local_origin {
                            text.push_str(&format!("Local source: `{}`\n\nPublication: `{}`\n\nAuthorization revision: {}\n\n",origin.source_id,origin.publication_id,origin.authorization_revision));
                        }
                        text.push_str(&format!("Payload: `{}`\n\nPath: {}\n\nLines: {} (count {})\n\nContext checksum: `{}`\n\n",evidence.payload_hash,Self::escape(&evidence.path),evidence.start_line,evidence.line_count,evidence.context_checksum));
                    }
                }

                ("text/markdown", text)
            }
            _ => {
                return Err(PlatformError::Validation(
                    "document export supports json or markdown",
                ));
            }
        };
        Ok(DocumentExport {
            document_id: version.document_id.clone(),
            version_id: version.id.clone(),
            checksum: version.checksum.clone(),
            media_type: media_type.into(),
            content,
        })
    }
    fn section(output: &mut String, value: &Value, level: usize) {
        match value {
            Value::Object(fields) => {
                let mut fields = fields.iter().collect::<Vec<_>>();
                fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
                for (key, value) in fields {
                    if value.is_null() {
                        continue;
                    }
                    output.push_str(&format!(
                        "{} {}\n\n",
                        "#".repeat(level.min(6)),
                        Self::escape(&key.replace('_', " "))
                    ));
                    Self::section(output, value, level + 1);
                }
            }
            Value::Array(items) => {
                if items.is_empty() {
                    output.push_str("None declared.\n\n");
                }
                for item in items {
                    if let Some(text) = item.as_str() {
                        output.push_str(&format!("- {}\n\n", Self::escape(text)));
                    }
                }
            }
            Value::String(text) => output.push_str(&format!("{}\n\n", Self::escape(text))),
            _ => {}
        }
    }
    fn escape(value: &str) -> String {
        let mut text = String::new();
        for ch in value.chars() {
            match ch {
                '&' => text.push_str("&amp;"),
                '<' => text.push_str("&lt;"),
                '>' => text.push_str("&gt;"),
                '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-'
                | '!' | '|' => {
                    text.push('\\');
                    text.push(ch);
                }
                other => text.push(other),
            }
        }
        text
    }
}

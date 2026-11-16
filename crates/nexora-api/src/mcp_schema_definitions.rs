use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// Ships the transitive closure of a tool's local input references, without unrelated models.
pub(crate) struct McpSchemaDefinitions;
impl McpSchemaDefinitions {
    pub fn attach(tools: &mut [Value], definitions: &Value) {
        for tool in tools {
            let schema = &mut tool["inputSchema"];
            schema
                .as_object_mut()
                .expect("bundled tool input schema is an object")
                .remove("$defs");
            let mut pending = Vec::new();
            Self::references(schema, &mut pending);
            let mut needed = BTreeSet::new();
            while let Some(name) = pending.pop() {
                if needed.insert(name.clone()) {
                    let definition = definitions
                        .get(&name)
                        .expect("bundled input schema reference exists");
                    Self::references(definition, &mut pending);
                }
            }
            if !needed.is_empty() {
                let selected: Map<String, Value> = needed
                    .into_iter()
                    .map(|name| {
                        let definition = definitions
                            .get(&name)
                            .expect("bundled input schema reference exists")
                            .clone();
                        (name, definition)
                    })
                    .collect();
                schema["$defs"] = Value::Object(selected);
            }
        }
    }
    fn references(value: &Value, pending: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                if let Some(name) = fields
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|reference| reference.strip_prefix("#/$defs/"))
                {
                    pending.push(name.into());
                }
                for value in fields.values() {
                    Self::references(value, pending);
                }
            }
            Value::Array(items) => {
                for value in items {
                    Self::references(value, pending);
                }
            }
            _ => {}
        }
    }
}

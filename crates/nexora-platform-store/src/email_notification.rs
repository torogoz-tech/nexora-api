/// Internal delivery claim. Never exposed by REST or MCP.
pub struct EmailNotification {
    pub event_key: String,
    pub kind: String,
    pub project_id: Option<String>,
    pub resource_id: String,
    pub actor_id: String,
    pub email: String,
    pub attempts: i32,
    pub language: String,
}

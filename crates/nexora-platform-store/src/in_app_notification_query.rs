use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InAppNotificationQuery {
    #[serde(default = "InAppNotificationQuery::default_folder")]
    pub folder: String,
    #[serde(default = "InAppNotificationQuery::default_limit")]
    pub limit: u16,
    #[serde(default)]
    pub offset: u32,
}
impl InAppNotificationQuery {
    fn default_folder() -> String {
        "inbox".into()
    }
    fn default_limit() -> u16 {
        50
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InAppNotificationChange {
    pub read: Option<bool>,
    pub archived: Option<bool>,
}

use nexora_platform::ApprovalRequestStatus;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApprovalRequestPageQuery {
    pub after: Option<String>,
    pub limit: Option<u16>,
    pub status: Option<ApprovalRequestStatus>,
}

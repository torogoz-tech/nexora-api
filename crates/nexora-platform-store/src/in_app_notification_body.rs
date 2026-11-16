use serde_json::Value;
pub(crate) fn in_app_notification_body(value: &Value, spanish: bool) -> String {
    let title = value["resource_title"].as_str().unwrap_or_default();
    let sender = value["sender_name"].as_str().unwrap_or_default();
    let date = value["event_date"].as_str().unwrap_or_default();
    let status = match value["decision"].as_str().unwrap_or_default() {
        "approved" => {
            if spanish {
                "Aprobado"
            } else {
                "Approved"
            }
        }
        "rejected" => {
            if spanish {
                "Rechazado"
            } else {
                "Rejected"
            }
        }
        "cancelled" => {
            if spanish {
                "Cancelado"
            } else {
                "Cancelled"
            }
        }
        _ => "",
    };
    let mut parts = Vec::new();
    if !sender.is_empty() {
        parts.push(sender.to_owned());
    }
    if !title.is_empty() {
        parts.push(title.to_owned());
    }
    if !date.is_empty() {
        parts.push(date.to_owned());
    }
    if !status.is_empty() {
        parts.push(status.to_owned());
    }
    parts.join(" · ")
}

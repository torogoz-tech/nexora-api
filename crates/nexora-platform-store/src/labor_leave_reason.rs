use crate::labor_calendar_policy::LaborCalendarPolicy;
use nexora_platform::PlatformError;
use serde_json::Value;

pub(crate) fn labor_leave_reason<'a>(
    input: &'a Value,
    kind: &str,
) -> Result<&'a str, PlatformError> {
    if kind != "vacation" {
        return LaborCalendarPolicy::text(input, "reason", 4000);
    }
    let reason = match input.get("reason") {
        None | Some(Value::Null) => "",
        Some(Value::String(value)) => value.trim(),
        _ => return Err(PlatformError::Validation("invalid leave reason")),
    };
    if reason.len() > 4000 || reason.chars().any(|c| c.is_control() && c != '\n') {
        return Err(PlatformError::Validation("invalid leave reason"));
    }
    Ok(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn vacation_does_not_require_reason_but_leave_does() {
        for input in [json!({}), json!({"reason": null}), json!({"reason": ""})] {
            assert_eq!(labor_leave_reason(&input, "vacation").unwrap(), "");
            assert!(labor_leave_reason(&input, "leave").is_err());
        }
        assert!(labor_leave_reason(&json!({"reason": 12}), "vacation").is_err());
        assert!(labor_leave_reason(&json!({"reason": "x".repeat(4001)}), "vacation").is_err());
        assert_eq!(
            labor_leave_reason(&json!({"reason": "Appointment"}), "leave").unwrap(),
            "Appointment"
        );
    }
}

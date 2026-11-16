use nexora_platform::PlatformError;
use serde_json::{Value, json};
pub(crate) struct WorkingHoursPolicy;
impl WorkingHoursPolicy {
    pub fn clock(value: &Value) -> Result<i32, PlatformError> {
        let s = value
            .as_str()
            .filter(|s| s.len() == 5 && s.as_bytes()[2] == b':')
            .ok_or(PlatformError::Validation("time must use HH:mm"))?;
        if !s
            .as_bytes()
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || b.is_ascii_digit())
        {
            return Err(PlatformError::Validation("invalid time"));
        }
        let h = s[..2]
            .parse::<i32>()
            .map_err(|_| PlatformError::Validation("invalid hour"))?;
        let m = s[3..]
            .parse::<i32>()
            .map_err(|_| PlatformError::Validation("invalid minute"))?;
        if h > 23 || m > 59 {
            return Err(PlatformError::Validation("invalid time"));
        }
        Ok(h * 60 + m)
    }
    pub fn settings(current: &Value, input: &Value) -> Result<Value, PlatformError> {
        let mut result = current.clone();
        let fields = ["work_start", "work_end", "break_minutes"];
        let count = fields.iter().filter(|k| input.get(**k).is_some()).count();
        if count != 0 && count != 3 {
            return Err(PlatformError::Validation(
                "working hours fields must be supplied together",
            ));
        }
        if let Some(object) = input.as_object() {
            for (key, value) in object {
                if key != "expected_revision" && key != "revision" {
                    result[key] = value.clone()
                }
            }
        } else {
            return Err(PlatformError::Validation("invalid calendar settings"));
        }
        let start = Self::clock(&result["work_start"])?;
        if count == 0
            && let Some(minutes) = input.get("daily_minutes")
        {
            let minutes = minutes
                .as_i64()
                .filter(|v| (1..=1440).contains(v))
                .ok_or(PlatformError::Validation("invalid daily minutes"))?;
            let pause = result["break_minutes"]
                .as_i64()
                .ok_or(PlatformError::Validation("invalid break minutes"))?;
            if minutes + pause > 1440 {
                return Err(PlatformError::Validation("workday exceeds 24 hours"));
            }
            let end = (i64::from(start) + minutes + pause) % 1440;
            result["work_end"] = json!(format!("{:02}:{:02}", end / 60, end % 60));
        }
        let end = Self::clock(&result["work_end"])?;
        let duration = end - start + if end <= start { 1440 } else { 0 };
        let pause = result["break_minutes"]
            .as_i64()
            .filter(|v| *v >= 0 && *v < i64::from(duration))
            .ok_or(PlatformError::Validation(
                "break must be shorter than workday",
            ))?;
        let minutes = i64::from(duration) - pause;
        if count == 3
            && input
                .get("daily_minutes")
                .is_some_and(|v| v.as_i64() != Some(minutes))
        {
            return Err(PlatformError::Validation(
                "daily minutes do not match working hours",
            ));
        }
        result["daily_minutes"] = json!(minutes);
        Ok(result)
    }
}

use crate::labor_calendar_policy::LaborCalendarPolicy;
use nexora_platform::PlatformError;
use serde_json::Value;
pub(crate) struct CalendarActivityInput;
impl CalendarActivityInput {
    pub fn validate(input: &Value, editing: bool) -> Result<(), PlatformError> {
        let fields = [
            "title",
            "mode",
            "attendance_required",
            "description",
            "location",
            "start_date",
            "end_date",
            "all_day",
            "start_time",
            "end_time",
            "expected_revision",
        ];
        let object = input
            .as_object()
            .ok_or(PlatformError::Validation("invalid calendar activity"))?;
        if object.keys().any(|k| !fields.contains(&k.as_str()))
            || (!editing && input.get("expected_revision").is_some())
        {
            return Err(PlatformError::Validation("invalid calendar activity field"));
        }
        LaborCalendarPolicy::text(input, "title", 200)?;
        for (key, max) in [("description", 4000), ("location", 500)] {
            if input.get(key).is_some_and(|v| {
                !v.as_str().is_some_and(|s| {
                    s.len() <= max && !s.chars().any(|c| c.is_control() && c != '\n')
                })
            }) {
                return Err(PlatformError::Validation("invalid calendar activity text"));
            }
        }
        if input
            .get("attendance_required")
            .is_some_and(|v| !v.is_boolean())
        {
            return Err(PlatformError::Validation("invalid attendance requirement"));
        }
        if let Some(mode) = input.get("mode").filter(|v| !v.is_null()) {
            if !mode
                .as_str()
                .is_some_and(|s| ["virtual", "in_person"].contains(&s))
            {
                return Err(PlatformError::Validation("invalid activity mode"));
            }
            let location = input["location"].as_str().unwrap_or("").trim();
            if location.is_empty() {
                return Err(PlatformError::Validation("activity location required"));
            }
            if mode == "virtual" {
                let link = url::Url::parse(location)
                    .map_err(|_| PlatformError::Validation("invalid activity meeting link"))?;
                if !["https", "http"].contains(&link.scheme())
                    || link.host_str().is_none()
                    || !link.username().is_empty()
                    || link.password().is_some()
                {
                    return Err(PlatformError::Validation("invalid activity meeting link"));
                }
            }
        }
        let start = LaborCalendarPolicy::date(input, "start_date")?;
        let end = LaborCalendarPolicy::date(input, "end_date")?;
        let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
        let days = (time::Date::parse(&end, &format).unwrap()
            - time::Date::parse(&start, &format).unwrap())
        .whole_days();
        if !(0..=365).contains(&days) {
            return Err(PlatformError::Validation("invalid calendar activity dates"));
        }
        let all_day = input["all_day"].as_bool().ok_or(PlatformError::Validation(
            "invalid calendar activity duration",
        ))?;
        if all_day {
            if !input["start_time"].is_null() || !input["end_time"].is_null() {
                return Err(PlatformError::Validation(
                    "all day activity cannot specify times",
                ));
            }
        } else {
            let format = time::format_description::parse_borrowed::<2>("[hour]:[minute]").unwrap();
            for key in ["start_time", "end_time"] {
                let text = input[key]
                    .as_str()
                    .filter(|s| s.len() == 5)
                    .ok_or(PlatformError::Validation("invalid calendar activity time"))?;
                time::Time::parse(text, &format)
                    .map_err(|_| PlatformError::Validation("invalid calendar activity time"))?;
            }
            if start == end && input["end_time"].as_str() <= input["start_time"].as_str() {
                return Err(PlatformError::Validation("activity must end after start"));
            }
        }
        Ok(())
    }
}

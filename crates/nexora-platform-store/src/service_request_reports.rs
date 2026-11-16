use crate::{PlatformStore, service_request_policy::ServiceRequestPolicy as Policy};
use nexora_platform::*;
use serde_json::Value;

impl PlatformStore {
    pub fn service_request_report(
        &mut self,
        ctx: &AuthContext,
        filters: &Value,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let role = Policy::role(&mut tx, ctx)?;
        let text = |key: &str| -> Result<&str, PlatformError> {
            let s = filters.get(key).and_then(Value::as_str).unwrap_or("");
            if s.len() > 300 {
                return Err(PlatformError::Validation("invalid request filter"));
            }
            Ok(s)
        };
        let from = text("from")?;
        let to = text("to")?;
        for date in [from, to] {
            if !date.is_empty()
                && time::Date::parse(
                    date,
                    &time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap(),
                )
                .is_err()
            {
                return Err(PlatformError::Validation("invalid report date"));
            }
        }
        if !from.is_empty() && !to.is_empty() && from > to {
            return Err(PlatformError::Validation("invalid report dates"));
        }
        let product = text("product_id")?;
        let kind = text("type_id")?;
        let level = text("criticality")?;
        let requester = text("requester_id")?;
        let status = text("status")?;
        if !matches!(status, "" | "new" | "in_progress" | "completed")
            || !matches!(level, "" | "low" | "medium" | "high" | "critical")
        {
            return Err(PlatformError::Validation("invalid request filter"));
        }
        let page = filters["page"].as_i64().unwrap_or(1);
        let offset = (page - 1)
            .checked_mul(20)
            .filter(|_| page > 0)
            .ok_or(PlatformError::Validation("invalid page"))?;
        let export = filters["export"].as_bool().unwrap_or(false);
        let result: Value = tx
            .query_one(
                include_str!("../sql/service_request_report.sql"),
                &[
                    &(role != "user"),
                    &ctx.actor_id(),
                    &from,
                    &to,
                    &product,
                    &kind,
                    &level,
                    &requester,
                    &status,
                    &offset,
                    &export,
                ],
            )
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn service_request_report_csv(report: &Value) -> String {
        fn cell(v: &Value) -> String {
            let s = v.as_str().map(str::to_owned).unwrap_or_else(|| {
                if v.is_null() {
                    String::new()
                } else {
                    v.to_string()
                }
            });
            let safe = if s.trim_start().starts_with(['=', '+', '-', '@']) {
                format!("'{s}")
            } else {
                s
            };
            format!("\"{}\"", safe.replace('"', "\"\""))
        }
        let mut csv = String::from(
            "\u{feff}Número,Título,Producto,Tipo,Solicitante,Criticidad,Estado,Creada,Primera atención,Resolución,SLA atención,SLA resolución\r\n",
        );
        if let Some(rows) = report["items"].as_array() {
            for row in rows {
                let values = [
                    &row["number"],
                    &row["title"],
                    &row["product_name"],
                    &row["type_name"],
                    &row["requester_name"],
                    &row["criticality"],
                    &row["status"],
                    &row["created_at"],
                    &row["first_attention_at"],
                    &row["resolved_at"],
                    &row["sla"]["response_status"],
                    &row["sla"]["resolution_status"],
                ];
                csv.push_str(&values.into_iter().map(cell).collect::<Vec<_>>().join(","));
                csv.push_str("\r\n");
            }
        }
        csv
    }
}

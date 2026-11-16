use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    service_request_policy::ServiceRequestPolicy as Policy,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn attach_service_request(
        &mut self,
        ctx: &AuthContext,
        request_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        if name.is_empty()
            || name.len() > 240
            || name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
            || bytes.is_empty()
            || bytes.len() > 20 * 1024 * 1024
        {
            return Err(PlatformError::Validation("invalid request attachment"));
        }
        let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let media = match extension.as_str() {
            "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
            "jpg" | "jpeg" if bytes.starts_with(&[255, 216, 255]) => "image/jpeg",
            "webp" if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
                "image/webp"
            }
            "pdf" if bytes.starts_with(b"%PDF-") => "application/pdf",
            "docx" | "xlsx" | "pptx" if bytes.starts_with(b"PK\x03\x04") => {
                "application/octet-stream"
            }
            "doc" | "xls" | "ppt" if bytes.starts_with(&[208, 207, 17, 224, 161, 177, 26, 225]) => {
                "application/octet-stream"
            }
            "txt" | "csv" if std::str::from_utf8(bytes).is_ok() => "text/plain",
            _ => {
                return Err(PlatformError::Validation(
                    "unsupported request attachment; use PNG, JPEG, WebP, PDF, Office, TXT or CSV",
                ));
            }
        };
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::platform_authority::PlatformAuthority::require(
            &mut tx,
            ctx,
            Capability::RequestsWrite,
            None,
        )?;
        Policy::visible(&mut tx, ctx, request_id, true)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "service_request.attach",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let id = SecretGenerator::id("sra")?;
        tx.execute("INSERT INTO service_request_attachments(id,request_id,actor_id,name,media_type,content) VALUES($1,$2,$3,$4,$5,$6)",&[&id,&request_id,&ctx.actor_id(),&name,&media,&bytes]).map_err(Self::database_error)?;
        tx.execute("INSERT INTO service_request_history(request_id,actor_id,action,detail) VALUES($1,$2,'attached',$3)",&[&request_id,&ctx.actor_id(),&json!({"name":name})]).map_err(Self::database_error)?;
        let result = json!({"id":id,"name":name,"media_type":media,"url":format!("/api/v1/requests/{request_id}/attachments/{id}")});
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn service_request_attachment(
        &mut self,
        ctx: &AuthContext,
        request_id: &str,
        id: &str,
    ) -> Result<(String, String, Vec<u8>), PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::visible(&mut tx, ctx, request_id, false)?;
        let row=tx.query_opt("SELECT name,media_type,content FROM service_request_attachments WHERE id=$1 AND request_id=$2",&[&id,&request_id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        let result = (row.get(0), row.get(1), row.get(2));
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
}

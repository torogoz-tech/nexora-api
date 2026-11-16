use nexora_platform::{ContextPackageExport, PlatformError, ProjectContextPackage};
pub(crate) struct ContextPackageRenderer;
impl ContextPackageRenderer {
    pub fn render(
        package: &ProjectContextPackage,
        format: &str,
    ) -> Result<ContextPackageExport, PlatformError> {
        let mut value = serde_json::to_value(package).map_err(|_| PlatformError::Storage)?;
        value
            .as_object_mut()
            .ok_or(PlatformError::Storage)?
            .remove("needs_review");
        let json = serde_json::to_string_pretty(&value).map_err(|_| PlatformError::Storage)?;
        let (media_type, content) = match format {
            "json" => ("application/json", json),
            "markdown" => {
                let longest = json.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
                let fence = "`".repeat((longest + 1).max(3));
                (
                    "text/markdown",
                    format!(
                        "# Project context package\n\nCaptured data for external actors. Mutable freshness is reported separately.\n\n{fence}json\n{json}\n{fence}\n"
                    ),
                )
            }
            _ => {
                return Err(PlatformError::Validation(
                    "context package export supports json or markdown",
                ));
            }
        };
        Ok(ContextPackageExport {
            package_id: package.id.clone(),
            checksum: package.checksum.clone(),
            media_type: media_type.into(),
            content,
        })
    }
}

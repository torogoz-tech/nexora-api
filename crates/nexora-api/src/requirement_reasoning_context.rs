use nexora_platform::{DocumentContent, PlatformError, ProjectDocumentVersion};
use nexora_reasoning::{AnalysisRequest, Criterion};
use serde_json::{Value, json};
/// Preserves business context and version provenance independently from HTTP coordination.
pub(crate) struct RequirementReasoningContext;
impl RequirementReasoningContext {
    pub fn prepare(
        analysis: &mut AnalysisRequest,
        document: &ProjectDocumentVersion,
    ) -> Result<Value, PlatformError> {
        if !matches!(
            analysis.purpose.as_str(),
            "requirement_connection" | "contribution_review"
        ) {
            return Err(PlatformError::Validation(
                "invalid purpose for requirement context",
            ));
        }
        if document.needs_review {
            return Err(PlatformError::Conflict("requirement context needs review"));
        }
        let content: DocumentContent =
            serde_json::from_value(document.content.clone()).map_err(|_| PlatformError::Storage)?;
        let DocumentContent::Requirement(body) = content else {
            return Err(PlatformError::Validation(
                "document version is not a requirement",
            ));
        };
        analysis.objective = format!(
            "Evaluate exact stored requirement {} version {}. Request: {}. Full structured document (facts and assumptions remain distinct): {}",
            document.document_id, document.id, analysis.objective, document.content
        );
        analysis.criteria = body
            .acceptance_criteria
            .into_iter()
            .enumerate()
            .map(|(index, description)| Criterion {
                id: format!("AC-{}", index + 1),
                description,
            })
            .collect();
        if analysis.purpose == "contribution_review" {
            analysis.criteria.push(Criterion{id:"CHANGE_QUALITY".into(),description:"Evaluar la calidad de los cambios seleccionados frente al requerimiento: corrección, claridad y mantenibilidad. Usar la comparación de snapshots; distinguir evidencia de pruebas ejecutadas y autoría no verificada.".into()});
        }
        analysis.validate().map_err(|_| {
            PlatformError::Validation("stored requirement exceeds reasoning limits")
        })?;
        Ok(
            json!({"document_id":document.document_id,"version_id":document.id,"checksum":document.checksum,"state_revision":document.state_revision,"state_id":document.state_id,"needs_review":document.needs_review,"context_manifest_id":document.context_manifest_id,"evidence":document.evidence,"origin":document.origin}),
        )
    }
    pub fn revalidate(
        initial: &Value,
        current: &ProjectDocumentVersion,
    ) -> Result<(), PlatformError> {
        if initial["document_id"] != current.document_id
            || initial["version_id"] != current.id
            || initial["checksum"] != current.checksum
            || initial["state_revision"] != current.state_revision
            || initial["state_id"] != current.state_id
            || current.needs_review
            || initial["context_manifest_id"] != json!(current.context_manifest_id)
        {
            return Err(PlatformError::Conflict(
                "requirement changed during analysis",
            ));
        }
        Ok(())
    }
}

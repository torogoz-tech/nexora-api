use nexora_reasoning::{
    AnalysisRequest, ComparisonTool, Criterion, OpenAiClient, ReasoningSession, SecretFile,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("private key file required")?;
    let client = OpenAiClient::new(
        SecretFile::read(std::path::Path::new(&path))?,
        "gpt-6.1-sol".into(),
    )?;
    let request=AnalysisRequest{purpose:"contribution_review".into(),objective:"Evalúa en español un cambio ficticio en payment.rs. El requerimiento exige rechazar negativos y aceptar cero. Compara la función anterior y actual usando la herramienta. La autoría no está verificada y no se ejecutaron pruebas.".into(),criteria:vec![Criterion{id:"AC-1".into(),description:"El cambio rechaza importes negativos".into()},Criterion{id:"AC-2".into(),description:"El cambio preserva la aceptación del importe cero".into()},Criterion{id:"CHANGE_QUALITY".into(),description:"El cambio satisface el requerimiento sin introducir regresiones lógicas".into()}]};
    let started = std::time::Instant::now();
    let (report,evidence)=ReasoningSession::new(client).evaluate(&request,vec![ComparisonTool::definition()],|name,_,_| {
        if name!=ComparisonTool::NAME {return Err(nexora_reasoning::ReasoningError::Repository)}
        Ok(json!({"synthetic":true,"comparison":"selected_source_windows","before":{"snapshot":"baseline-synthetic","path":"payment.rs","source":"fn accept(amount:i64)->bool { true }"},"after":{"snapshot":"current-synthetic","path":"payment.rs","source":"fn accept(amount:i64)->bool { amount > 0 }"},"authorship":"not_independently_verified"}))
    })?;
    println!(
        "{}",
        json!({"synthetic":true,"elapsed_ms":started.elapsed().as_millis(),"report":report,"evidence":evidence})
    );
    Ok(())
}

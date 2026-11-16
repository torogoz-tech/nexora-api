use nexora_reasoning::{
    AnalysisRequest, Criterion, OpenAiClient, ReasoningSession, RegisteredRepository, SecretFile,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("private key file required")?;
    let client = OpenAiClient::new(
        SecretFile::read(std::path::Path::new(&path))?,
        "gpt-6.1-sol".into(),
    )?;
    let request=AnalysisRequest{purpose:"requirement_connection".into(),objective:"Check the synthetic payment guard. The only fictitious path is payment.rs. Do not claim tests ran.".into(),criteria:vec![Criterion{id:"C1".into(),description:"Negative amounts must be rejected".into()},Criterion{id:"C2".into(),description:"Zero must be rejected".into()},Criterion{id:"C3".into(),description:"Tests have been executed successfully".into()}]};
    let started = std::time::Instant::now();
    let (report,evidence)=ReasoningSession::new(client).evaluate(&request,RegisteredRepository::tools(),|_,_,_|Ok(json!({"path":"payment.rs","start_line":1,"source":"fn accept(amount:i64)->bool { amount >= 0 }","synthetic":true})))?;
    println!(
        "{}",
        json!({"synthetic":true,"elapsed_ms":started.elapsed().as_millis(),"report":report,"evidence":evidence})
    );
    Ok(())
}

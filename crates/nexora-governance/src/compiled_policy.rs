#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub policy: crate::Policy,
    pub target: crate::CompiledTarget,
    pub plan: crate::EvaluationPlan,
    pub fingerprint: String,
}

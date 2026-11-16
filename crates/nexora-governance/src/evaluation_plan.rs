#[derive(Debug, Clone)]
pub enum EvaluationPlan {
    All(Vec<Self>),
    Any(Vec<Self>),
    None(Vec<Self>),
    Attribute(crate::CompiledAttribute),
    Relation(crate::CompiledRelation),
}

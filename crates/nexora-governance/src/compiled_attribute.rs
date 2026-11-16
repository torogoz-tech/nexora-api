use regex::Regex;
#[derive(Debug, Clone)]
pub struct CompiledAttribute {
    pub attribute: String,
    pub operator: String,
    pub value: Option<String>,
    pub pattern: Option<Regex>,
}

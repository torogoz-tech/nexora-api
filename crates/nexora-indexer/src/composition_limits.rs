use nexora_core::NexoraError;
#[derive(Debug, Clone)]
pub struct CompositionLimits {
    pub max_units: usize,
    pub max_work_bytes: usize,
    pub max_tokens: usize,
    pub max_nodes: usize,
    pub max_parser_steps: usize,
}
impl Default for CompositionLimits {
    fn default() -> Self {
        Self {
            max_units: 1024,
            max_work_bytes: 8_388_608,
            max_tokens: 200_000,
            max_nodes: 200_000,
            max_parser_steps: 64_000_000,
        }
    }
}
impl CompositionLimits {
    pub(crate) fn validate(&self) -> Result<(), NexoraError> {
        let ceiling = Self::default();
        if [
            (self.max_units, ceiling.max_units),
            (self.max_work_bytes, ceiling.max_work_bytes),
            (self.max_tokens, ceiling.max_tokens),
            (self.max_nodes, ceiling.max_nodes),
            (self.max_parser_steps, ceiling.max_parser_steps),
        ]
        .iter()
        .any(|(value, max)| *value == 0 || value > max)
        {
            return Err(NexoraError::Configuration(
                "invalid composition limits".into(),
            ));
        }
        Ok(())
    }
}

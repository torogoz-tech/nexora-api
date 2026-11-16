use nexora_core::NexoraError;
pub(crate) struct LexicalRegionBudget {
    pub steps: usize,
    pub nodes: usize,
    pub started: std::time::Instant,
}
impl LexicalRegionBudget {
    pub fn spend(&mut self) -> Result<(), NexoraError> {
        if self.steps == 0 || self.started.elapsed().as_secs() >= 5 {
            return Err(NexoraError::Limit(
                "lexical region operation/time budget".into(),
            ));
        }
        self.steps -= 1;
        Ok(())
    }
    pub fn node(&mut self) -> Result<(), NexoraError> {
        if self.nodes == 0 {
            return Err(NexoraError::Limit("lexical region node budget".into()));
        }
        self.nodes -= 1;
        Ok(())
    }
}

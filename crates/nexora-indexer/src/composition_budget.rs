use crate::CompositionLimits;
use nexora_core::NexoraError;
pub(crate) struct CompositionBudget {
    limits: CompositionLimits,
    units: usize,
    bytes: usize,
    steps: usize,
    tokens: usize,
    nodes: usize,
}
impl CompositionBudget {
    pub fn new(limits: CompositionLimits) -> Self {
        Self {
            limits,
            units: 0,
            bytes: 0,
            steps: 0,
            tokens: 0,
            nodes: 0,
        }
    }
    pub fn account_bytes(&mut self, bytes: usize) -> Result<(), NexoraError> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > self.limits.max_work_bytes {
            return Err(NexoraError::Limit("composition work bytes budget".into()));
        }
        Ok(())
    }
    pub fn admit(&mut self, bytes: usize) -> Result<(), NexoraError> {
        self.units = self.units.saturating_add(1);
        if self.units > self.limits.max_units {
            return Err(NexoraError::Limit("composition units budget".into()));
        }
        self.account_bytes(bytes)
    }
    pub fn remaining_parser_steps(&self) -> usize {
        self.limits.max_parser_steps.saturating_sub(self.steps)
    }
    pub fn account_parser_steps(&mut self, steps: usize) -> Result<(), NexoraError> {
        self.steps = self.steps.saturating_add(steps);
        if self.steps > self.limits.max_parser_steps {
            return Err(NexoraError::Limit(
                "composition parser operations budget".into(),
            ));
        }
        Ok(())
    }
    pub fn account(&mut self, tokens: usize, nodes: usize) -> Result<(), NexoraError> {
        self.tokens = self.tokens.saturating_add(tokens);
        self.nodes = self.nodes.saturating_add(nodes);
        if self.tokens > self.limits.max_tokens || self.nodes > self.limits.max_nodes {
            return Err(NexoraError::Limit("composition tokens/nodes budget".into()));
        }
        Ok(())
    }
}

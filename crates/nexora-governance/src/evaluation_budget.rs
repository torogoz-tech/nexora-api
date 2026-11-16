pub struct EvaluationBudget {
    remaining: usize,
}
impl Default for EvaluationBudget {
    fn default() -> Self {
        Self { remaining: 200_000 }
    }
}
impl EvaluationBudget {
    pub fn consume(&mut self) -> bool {
        if self.remaining == 0 {
            false
        } else {
            self.remaining -= 1;
            true
        }
    }
}

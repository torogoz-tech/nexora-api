use nexora_core::NexoraError;
use regex::Regex;
use std::collections::BTreeMap;
#[derive(Default)]
pub(crate) struct LexicalRegexCache {
    patterns: BTreeMap<String, Regex>,
}
impl LexicalRegexCache {
    pub fn compile(&mut self, pattern: &str) -> Result<Regex, NexoraError> {
        if let Some(compiled) = self.patterns.get(pattern) {
            return Ok(compiled.clone());
        }
        if self.patterns.len() >= 512 {
            return Err(NexoraError::Limit(
                "lexical regex cache > 512 patterns".into(),
            ));
        }
        let compiled = regex::RegexBuilder::new(&format!("^(?:{pattern})"))
            .size_limit(1_000_000)
            .build()
            .map_err(|error| NexoraError::Configuration(error.to_string()))?;
        self.patterns.insert(pattern.into(), compiled.clone());
        Ok(compiled)
    }
    #[cfg(test)]
    pub fn compiled_patterns(&self) -> usize {
        self.patterns.len()
    }
}

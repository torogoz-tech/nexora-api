use crate::{Token, compiled_token_rule::CompiledTokenRule};
use nexora_core::{NexoraError, SourcePosition, SourceSpan};
use nexora_language::LexerDefinition;
pub struct Lexer {
    rules: Vec<CompiledTokenRule>,
    rule_set: regex::RegexSet,
    entry_contexts: std::collections::BTreeMap<String, (Vec<CompiledTokenRule>, regex::RegexSet)>,
    identifier_kind: String,
    keywords: Vec<String>,
    keyword_after_exclusions: Vec<nexora_language::TokenSelector>,
    regions: crate::lexical_region_scanner::LexicalRegionScanner,
    max_region_steps: usize,
    max_region_nodes: usize,
}
impl Lexer {
    pub fn new(definition: &LexerDefinition) -> Result<Self, NexoraError> {
        nexora_language::LexerContextValidator::validate(definition)?;
        let mut rules = Vec::new();
        for rule in &definition.tokens {
            let regex = regex::RegexBuilder::new(&format!("^(?:{})", rule.pattern))
                .size_limit(1_000_000)
                .build()
                .map_err(|e| NexoraError::Configuration(e.to_string()))?;
            if regex.is_match("") {
                return Err(NexoraError::Configuration("empty token match".into()));
            }
            rules.push(CompiledTokenRule {
                follow_exclude: rule
                    .follow_exclude
                    .as_ref()
                    .map(|pattern| {
                        regex::RegexBuilder::new(&format!("^(?:{pattern})"))
                            .size_limit(1_000_000)
                            .build()
                            .map_err(|e| NexoraError::Configuration(e.to_string()))
                    })
                    .transpose()?,
                kind: rule.kind.clone(),
                root: rule.root,
                regex,
                region: rule.region.clone(),
                after: rule.after.clone(),
                after_kinds: rule.after_kinds.clone(),
                at_start: rule.at_start,
                reject_on_context_mismatch: rule.reject_on_context_mismatch,
                trivia: rule.trivia,
                reject: rule.reject,
            });
        }
        let regions = crate::lexical_region_scanner::LexicalRegionScanner::new(definition, &rules)?;
        let entry_contexts = definition
            .entry_contexts
            .iter()
            .map(|(name, kinds)| {
                let context_rules: Vec<_> = rules
                    .iter()
                    .filter(|rule| kinds.contains(&rule.kind))
                    .cloned()
                    .collect();
                let rule_set = compile_rule_set(&context_rules)?;
                Ok((name.clone(), (context_rules, rule_set)))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, NexoraError>>()?;
        let rules: Vec<_> = rules.into_iter().filter(|rule| rule.root).collect();
        let rule_set = compile_rule_set(&rules)?;
        Ok(Self {
            entry_contexts,
            rules,
            rule_set,
            regions,
            keyword_after_exclusions: definition.keyword_after_exclusions.clone(),
            max_region_steps: definition.max_region_steps,
            max_region_nodes: definition.max_region_nodes,
            identifier_kind: definition.identifier_kind.clone(),
            keywords: definition.keywords.clone(),
        })
    }
    pub fn tokenize(&self, source: &str) -> Result<Vec<Token>, NexoraError> {
        self.tokenize_range(source, 0..source.len())
    }
    pub fn tokenize_in_context(
        &self,
        source: &str,
        context: &str,
    ) -> Result<Vec<Token>, NexoraError> {
        self.tokenize_range_in_context(source, 0..source.len(), Some(context))
    }
    /// Tokenizes a bounded part of the original file. All emitted positions remain
    /// absolute; matching (including region closing and lookahead) ends at range.end.
    pub fn tokenize_range(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
    ) -> Result<Vec<Token>, NexoraError> {
        self.tokenize_range_in_context(source, range, None)
    }
    pub fn tokenize_range_in_context(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        context: Option<&str>,
    ) -> Result<Vec<Token>, NexoraError> {
        if source.get(range.clone()).is_none() {
            return Err(NexoraError::Configuration(
                "lexer range must be ordered, in bounds and on UTF-8 boundaries".into(),
            ));
        }
        let mut position = SourcePosition::default();
        position.advance(&source[..range.start]);
        self.tokenize_from(source, range, position, self.rules_for(context)?)
    }
    /// Uses the source-owned index to preserve absolute positions without prefix scans.
    pub fn tokenize_indexed_range(
        &self,
        index: &nexora_core::SourcePositionIndex<'_>,
        range: std::ops::Range<usize>,
    ) -> Result<Vec<Token>, NexoraError> {
        self.tokenize_indexed_range_in_context(index, range, None)
    }
    pub fn tokenize_indexed_range_in_context(
        &self,
        index: &nexora_core::SourcePositionIndex<'_>,
        range: std::ops::Range<usize>,
        context: Option<&str>,
    ) -> Result<Vec<Token>, NexoraError> {
        let position = index.position(range.start)?;
        self.tokenize_from(index.source(), range, position, self.rules_for(context)?)
    }
    fn rules_for(
        &self,
        context: Option<&str>,
    ) -> Result<(&[CompiledTokenRule], &regex::RegexSet), NexoraError> {
        match context {
            None => Ok((&self.rules, &self.rule_set)),
            Some(name) => self
                .entry_contexts
                .get(name)
                .map(|(rules, rule_set)| (rules.as_slice(), rule_set))
                .ok_or_else(|| {
                    NexoraError::Configuration(format!("unknown lexical entry context {name}"))
                }),
        }
    }
    fn tokenize_from(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        mut position: SourcePosition,
        rules: (&[CompiledTokenRule], &regex::RegexSet),
    ) -> Result<Vec<Token>, NexoraError> {
        if source.get(range.clone()).is_none() {
            return Err(NexoraError::Configuration(
                "lexer range must be ordered, in bounds and on UTF-8 boundaries".into(),
            ));
        }
        let source = &source[..range.end];
        let mut tokens: Vec<Token> = Vec::new();
        let mut previous: Option<usize> = None;
        let mut region_budget = crate::lexical_region_budget::LexicalRegionBudget {
            steps: self.max_region_steps,
            nodes: self.max_region_nodes,
        };
        while position.offset < source.len() {
            if tokens.len() >= 200_000 {
                return Err(NexoraError::Limit("lexer token budget".into()));
            }
            let remaining = &source[position.offset..];
            let context = previous.map(|index| crate::token_context::TokenContext {
                kind: tokens[index].kind.as_str(),
                text: tokens[index].text.as_str(),
            });
            let (rule, length) = crate::token_rule_selector::TokenRuleSelector::select(
                rules.0,
                rules.1,
                remaining,
                position.offset,
                context,
                |mode, suffix| self.regions.accepts(mode, suffix),
                None,
            )?;
            let (length, regions) = if let Some(mode) = &rule.region {
                let scanned =
                    self.regions
                        .scan(source, position, &rule.regex, mode, &mut region_budget)?;
                (scanned.length, scanned.regions)
            } else {
                (length, Vec::new())
            };
            let text = &remaining[..length];
            if rule.reject {
                return Err(NexoraError::Lexical(format!(
                    "rejected {} at byte {}",
                    rule.kind, position.offset
                )));
            }
            let start = position;
            position.advance(text);
            let context = previous.map(|index| crate::token_context::TokenContext {
                kind: tokens[index].kind.as_str(),
                text: tokens[index].text.as_str(),
            });
            let kind = crate::keyword_classifier::KeywordClassifier::classify(
                &rule.kind,
                text,
                context,
                &self.identifier_kind,
                &self.keywords,
                &self.keyword_after_exclusions,
            );
            if !rule.trivia {
                previous = Some(tokens.len());
            }
            tokens.push(Token {
                kind,
                text: text.into(),
                span: SourceSpan {
                    start,
                    end: position,
                },
                trivia: rule.trivia,
                regions,
            });
        }
        Ok(tokens)
    }
}

pub(crate) fn compile_rule_set(
    rules: &[CompiledTokenRule],
) -> Result<regex::RegexSet, NexoraError> {
    regex::RegexSetBuilder::new(rules.iter().map(|rule| rule.regex.as_str()))
        .size_limit(1_000_000)
        .build()
        .map_err(|error| NexoraError::Configuration(error.to_string()))
}

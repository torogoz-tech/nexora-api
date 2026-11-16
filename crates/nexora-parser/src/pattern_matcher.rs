use crate::{CaptureRange, MatchFailure};
use nexora_language::{GrammarDefinition, GrammarPattern, TokenSelector};
use nexora_lexer::Token;
use std::collections::BTreeMap;
pub struct PatternMatcher<'a> {
    pub grammar: &'a GrammarDefinition,
    pub tokens: &'a [(usize, &'a Token)],
    steps: usize,
    step_limit: usize,
    budget_exhausted: bool,
    source_tokens: Option<&'a [Token]>,
    pub(crate) context_start: usize,
    receiver_policies: BTreeMap<
        usize,
        std::sync::Arc<
            crate::compiled_expression_receiver_policy::CompiledExpressionReceiverPolicy,
        >,
    >,
}
impl<'a> PatternMatcher<'a> {
    pub fn new(grammar: &'a GrammarDefinition, tokens: &'a [(usize, &'a Token)]) -> Self {
        Self::with_step_limit(grammar, tokens, grammar.max_steps)
    }
    pub fn with_step_limit(
        grammar: &'a GrammarDefinition,
        tokens: &'a [(usize, &'a Token)],
        limit: usize,
    ) -> Self {
        Self {
            grammar,
            tokens,
            steps: 0,
            step_limit: grammar.max_steps.min(limit),
            budget_exhausted: false,
            source_tokens: None,
            context_start: 0,
            receiver_policies: BTreeMap::new(),
        }
    }
    pub fn with_source_tokens(
        grammar: &'a GrammarDefinition,
        tokens: &'a [(usize, &'a Token)],
        source_tokens: &'a [Token],
        limit: usize,
    ) -> Self {
        let mut matcher = Self::with_step_limit(grammar, tokens, limit);
        matcher.source_tokens = Some(source_tokens);
        matcher
    }
    pub fn steps(&self) -> usize {
        self.steps
    }
    pub(crate) fn receiver_policy(
        &mut self,
        definition: &nexora_language::ExpressionDefinition,
        at: usize,
    ) -> Result<
        Option<
            std::sync::Arc<
                crate::compiled_expression_receiver_policy::CompiledExpressionReceiverPolicy,
            >,
        >,
        MatchFailure,
    > {
        use crate::compiled_expression_receiver_policy::CompiledExpressionReceiverPolicy as Policy;
        if definition.application.len() > 16 {
            return Err(Policy::failure(at));
        }
        if definition
            .application
            .iter()
            .all(|a| a.excluded_receivers.is_empty())
        {
            return Ok(None);
        }
        // Definition identity is only a local immutable memo key, never graph evidence.
        let key = definition as *const nexora_language::ExpressionDefinition as usize;
        if let Some(policy) = self.receiver_policies.get(&key) {
            return Ok(Some(policy.clone()));
        }
        let policy = std::sync::Arc::new(Policy::compile(definition, self, at)?);
        self.receiver_policies.insert(key, policy.clone());
        Ok(Some(policy))
    }
    pub fn budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }
    fn failure(at: usize, message: &str, fatal: bool) -> MatchFailure {
        MatchFailure {
            at,
            message: message.into(),
            fatal,
        }
    }
    pub fn spend(&mut self, at: usize) -> Result<(), MatchFailure> {
        if self.steps >= self.step_limit {
            self.budget_exhausted = true;
            Err(Self::failure(at, "parser operation budget exhausted", true))
        } else {
            self.steps += 1;
            Ok(())
        }
    }
    pub fn selected(token: &Token, selector: &TokenSelector) -> bool {
        token.text == selector.text && selector.kind.as_ref().is_none_or(|k| *k == token.kind)
    }
    pub fn pattern(
        &mut self,
        pattern: &GrammarPattern,
        start: usize,
        captures: &mut BTreeMap<String, CaptureRange>,
        depth: usize,
    ) -> Result<usize, MatchFailure> {
        self.spend(start)?;
        if depth > self.grammar.max_nesting {
            return Err(Self::failure(start, "pattern nesting budget", true));
        }
        match pattern {
            GrammarPattern::Literal { text, kind } => {
                if self.tokens.get(start).is_some_and(|(_, t)| {
                    t.text == *text && kind.as_ref().is_none_or(|k| *k == t.kind)
                }) {
                    Ok(start + 1)
                } else {
                    Err(Self::failure(
                        start,
                        &format!("expected terminal {text:?}"),
                        false,
                    ))
                }
            }
            GrammarPattern::Token { kind } => {
                if self.tokens.get(start).is_some_and(|(_, t)| t.kind == *kind) {
                    Ok(start + 1)
                } else {
                    Err(Self::failure(
                        start,
                        &format!("expected token kind {kind}"),
                        false,
                    ))
                }
            }
            GrammarPattern::GapWithout { characters } => {
                if start <= self.context_start || start >= self.tokens.len() {
                    return Ok(start);
                }
                let left = self.tokens[start - 1].0 + 1;
                let right = self.tokens[start].0;
                let gap = self
                    .source_tokens
                    .and_then(|tokens| tokens.get(left..right))
                    .ok_or_else(|| {
                        Self::failure(
                            start,
                            "gap constraint requires the original token stream",
                            true,
                        )
                    })?;
                for token in gap {
                    for character in token.text.chars() {
                        self.spend(start)?;
                        if characters.contains(character) {
                            return Err(Self::failure(
                                start,
                                "forbidden character between significant tokens",
                                false,
                            ));
                        }
                    }
                }
                Ok(start)
            }
            GrammarPattern::EndOfContext => {
                if start == self.tokens.len() {
                    Ok(start)
                } else {
                    Err(Self::failure(
                        start,
                        "expected end of active context",
                        false,
                    ))
                }
            }
            GrammarPattern::LineEnd => {
                if start == self.tokens.len()
                    || (start > 0
                        && self.tokens.get(start).is_some_and(|(_, t)| {
                            t.span.start.line > self.tokens[start - 1].1.span.end.line
                        }))
                {
                    Ok(start)
                } else {
                    Err(Self::failure(start, "expected line break or EOF", false))
                }
            }
            GrammarPattern::Sequence { items } => {
                let mut cursor = start;
                for item in items {
                    cursor = self.pattern(item, cursor, captures, depth + 1)?;
                }
                Ok(cursor)
            }
            GrammarPattern::Choice { alternatives } => {
                let saved = captures.clone();
                let mut farthest = Self::failure(start, "no matching alternative", false);
                for alternative in alternatives {
                    *captures = saved.clone();
                    match self.pattern(alternative, start, captures, depth + 1) {
                        Ok(end) => return Ok(end),
                        Err(error) => {
                            if error.fatal {
                                return Err(error);
                            }
                            if error.at >= farthest.at {
                                farthest = error;
                            }
                        }
                    }
                }
                *captures = saved;
                Err(farthest)
            }
            GrammarPattern::Optional { item } => {
                let saved = captures.clone();
                match self.pattern(item, start, captures, depth + 1) {
                    Ok(end) => Ok(end),
                    Err(error) if error.fatal => Err(error),
                    Err(_) => {
                        *captures = saved;
                        Ok(start)
                    }
                }
            }
            GrammarPattern::Repeat { item, min, max } => {
                let mut cursor = start;
                let mut count = 0;
                while count < *max {
                    let saved = captures.clone();
                    match self.pattern(item, cursor, captures, depth + 1) {
                        Ok(end) if end > cursor => {
                            cursor = end;
                            count += 1;
                        }
                        Ok(_) => {
                            return Err(Self::failure(cursor, "repetition did not advance", true));
                        }
                        Err(error) if error.fatal => return Err(error),
                        Err(error) => {
                            *captures = saved;
                            if count < *min {
                                return Err(error);
                            }
                            break;
                        }
                    }
                }
                if count < *min {
                    Err(Self::failure(
                        cursor,
                        "minimum repetition count not reached",
                        false,
                    ))
                } else {
                    Ok(cursor)
                }
            }
            GrammarPattern::Capture { name, item, join } => {
                let end = self.pattern(item, start, captures, depth + 1)?;
                if end <= start || captures.len() >= 64 || captures.contains_key(name) {
                    return Err(Self::failure(start, "empty or duplicate capture", true));
                }
                captures.insert(
                    name.clone(),
                    CaptureRange {
                        start,
                        end,
                        join: join.clone(),
                    },
                );
                Ok(end)
            }
            GrammarPattern::Rule { name } => {
                let grammar = self.grammar;
                let rule = grammar
                    .rules
                    .get(name)
                    .ok_or_else(|| Self::failure(start, "unknown grammar rule", true))?;
                self.pattern(rule, start, captures, depth + 1)
            }
            GrammarPattern::ScanUntil { stop, allow_eof } => {
                let mut cursor = start;
                while cursor < self.tokens.len() {
                    self.spend(cursor)?;
                    let token = self.tokens[cursor].1;
                    if stop.iter().any(|s| Self::selected(token, s)) {
                        if cursor == start {
                            return Err(Self::failure(start, "empty scanned expression", false));
                        }
                        return Ok(cursor);
                    }
                    if let Some(group) = self.grammar.groups.iter().find(|g| {
                        g.open == token.text && g.kind.as_ref().is_none_or(|k| *k == token.kind)
                    }) {
                        cursor = self.balanced(cursor, &group.open)?;
                    } else if self.grammar.groups.iter().any(|g| {
                        g.close == token.text && g.kind.as_ref().is_none_or(|k| *k == token.kind)
                    }) {
                        return Err(Self::failure(
                            cursor,
                            "unexpected delimiter before scan boundary",
                            false,
                        ));
                    } else {
                        cursor += 1;
                    }
                }
                if *allow_eof && cursor > start {
                    Ok(cursor)
                } else {
                    Err(Self::failure(cursor, "missing scan boundary", false))
                }
            }
            GrammarPattern::Balanced { open } => self.balanced(start, open),
            GrammarPattern::Until { close } => self.region(start, close, None),
        }
    }
    pub fn balanced(&mut self, start: usize, open: &str) -> Result<usize, MatchFailure> {
        let group = self
            .grammar
            .groups
            .iter()
            .find(|g| {
                g.open == open
                    && self.tokens.get(start).is_some_and(|(_, t)| {
                        t.text == g.open && g.kind.as_ref().is_none_or(|k| *k == t.kind)
                    })
            })
            .ok_or_else(|| {
                Self::failure(start, &format!("expected configured group {open:?}"), false)
            })?;
        self.region(start + 1, &group.close, group.kind.as_deref())
    }
    fn region(
        &mut self,
        start: usize,
        close: &str,
        kind: Option<&str>,
    ) -> Result<usize, MatchFailure> {
        let kind = kind.or_else(|| {
            self.grammar
                .groups
                .iter()
                .find(|g| g.close == close)
                .and_then(|g| g.kind.as_deref())
        });
        let mut stack = vec![(close.to_owned(), kind.map(str::to_owned))];
        for index in start..self.tokens.len() {
            self.spend(index)?;
            let token = self.tokens[index].1;
            if stack.last().is_some_and(|(text, kind)| {
                *text == token.text && kind.as_ref().is_none_or(|k| *k == token.kind)
            }) {
                stack.pop();
                if stack.is_empty() {
                    return Ok(index + 1);
                }
                continue;
            }
            if let Some(group) =
                self.grammar.groups.iter().find(|g| {
                    g.open == token.text && g.kind.as_ref().is_none_or(|k| *k == token.kind)
                })
            {
                stack.push((group.close.clone(), group.kind.clone()));
                if stack.len() > self.grammar.max_nesting {
                    return Err(Self::failure(index, "group nesting budget exceeded", true));
                }
            } else if self
                .grammar
                .groups
                .iter()
                .any(|g| g.close == token.text && g.kind.as_ref().is_none_or(|k| *k == token.kind))
            {
                return Err(Self::failure(
                    index,
                    "mismatched configured delimiter",
                    false,
                ));
            }
        }
        Err(Self::failure(
            self.tokens.len(),
            "unterminated configured region",
            false,
        ))
    }
}

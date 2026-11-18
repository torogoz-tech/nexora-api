use crate::{
    compiled_lexical_region::CompiledLexicalRegion,
    compiled_lexical_transition::CompiledLexicalTransition,
    lexical_region_budget::LexicalRegionBudget,
    lexical_region_closing::LexicalRegionClosing as Closing,
    lexical_region_frame::LexicalRegionFrame, lexical_region_opening::LexicalRegionOpening,
    lexical_region_scan::LexicalRegionScan, resolved_counted_transition::ResolvedCountedTransition,
};
use nexora_core::{LexicalRegion, NexoraError, SourcePosition, SourceSpan};
use std::collections::BTreeMap;
pub(crate) struct LexicalRegionScanner {
    modes: BTreeMap<String, CompiledLexicalRegion>,
    max_depth: usize,
    identifier_kind: String,
    keywords: Vec<String>,
    keyword_after_exclusions: Vec<nexora_language::TokenSelector>,
}
impl LexicalRegionScanner {
    fn transitions(
        input: &[nexora_language::LexicalTransition],
        cache: &mut crate::lexical_regex_cache::LexicalRegexCache,
    ) -> Result<Vec<CompiledLexicalTransition>, NexoraError> {
        input
            .iter()
            .map(|c| {
                Ok(CompiledLexicalTransition {
                    regex: cache.compile(&c.pattern)?,
                    region: c.region.clone(),
                    reject: c.reject,
                })
            })
            .collect()
    }
    pub fn new(
        d: &nexora_language::LexerDefinition,
        rules: &[crate::compiled_token_rule::CompiledTokenRule],
    ) -> Result<Self, NexoraError> {
        let mut modes = BTreeMap::new();
        let mut cache = crate::lexical_regex_cache::LexicalRegexCache::default();
        for (name, definition) in &d.regions {
            let close_tail = definition
                .close_tail
                .as_ref()
                .map(|s| cache.compile(s))
                .transpose()?;
            let opening_follow = definition
                .opening_follow
                .as_ref()
                .map(|s| cache.compile(s))
                .transpose()?;
            let header_children = definition
                .header
                .as_ref()
                .map(|h| Self::transitions(&h.children, &mut cache))
                .transpose()?
                .unwrap_or_default();
            let token_rules: Vec<_> = rules
                .iter()
                .filter(|rule| definition.token_rules.contains(&rule.kind))
                .cloned()
                .collect();
            let token_rule_set = crate::lexer::compile_rule_set(&token_rules)?;
            modes.insert(
                name.clone(),
                CompiledLexicalRegion {
                    token_rules,
                    token_rule_set,
                    definition: definition.clone(),
                    children: Self::transitions(&definition.children, &mut cache)?,
                    header_children,
                    close_before: definition
                        .close_before
                        .iter()
                        .map(|b| {
                            Ok(crate::compiled_lexical_boundary::CompiledLexicalBoundary {
                                regex: cache.compile(&b.pattern)?,
                                after_kinds: b.after_kinds.clone(),
                            })
                        })
                        .collect::<Result<Vec<_>, NexoraError>>()?,
                    close_tail,
                    opening_follow,
                },
            );
        }
        Ok(Self {
            modes,
            max_depth: d.max_region_depth,
            identifier_kind: d.identifier_kind.clone(),
            keywords: d.keywords.clone(),
            keyword_after_exclusions: d.keyword_after_exclusions.clone(),
        })
    }
    pub fn accepts(&self, mode: &str, suffix: &str) -> bool {
        self.modes[mode]
            .opening_follow
            .as_ref()
            .is_none_or(|pattern| pattern.is_match(suffix))
    }
    fn pop_frame(stack: &mut Vec<LexicalRegionFrame>, regions: &[LexicalRegion]) {
        let frame = stack.pop().expect("completed frame");
        if let Some(parent) = stack.last_mut()
            && !frame.returned_trivia
        {
            parent.previous = Some(crate::context_token::ContextToken {
                kind: frame.returned_kind.unwrap_or_else(|| "region".into()),
                span: regions[frame.index].span,
            });
        }
    }
    fn finish(region: &mut LexicalRegion, before: SourcePosition, after: SourcePosition) {
        region.span.end = after;
        region.content_span.end = before;
        region.closer = Some(SourceSpan {
            start: before,
            end: after,
        });
        region.closed = true;
    }
    pub fn scan(
        &self,
        source: &str,
        start: SourcePosition,
        opening: &regex::Regex,
        mode: &str,
        budget: &mut LexicalRegionBudget,
    ) -> Result<LexicalRegionScan, NexoraError> {
        budget.node()?;
        let first = LexicalRegionOpening::new(
            source,
            start,
            opening,
            mode,
            &self.modes[mode].definition,
            None,
            budget,
        )?;
        let mut position = first.end;
        let mut stack = vec![LexicalRegionFrame {
            previous: None,
            returned_kind: self.modes[mode].definition.token_kind.clone(),
            returned_trivia: self.modes[mode].definition.trivia,
            mode: mode.into(),
            index: 0,
            header: self.modes[mode].definition.header.is_some(),
            close_value: first.close_value,
            terminal_value: None,
            counted_children: ResolvedCountedTransition::resolve(
                source,
                &first.region,
                &self.modes[mode].definition.counted_children,
                &self.modes,
                budget,
            )?,
        }];
        let mut regions = vec![first.region];
        'scan: while position.offset < source.len() {
            budget.spend()?;
            let frame = stack.last().expect("nonempty lexical stack");
            let spec = &self.modes[&frame.mode];
            let rest = &source[position.offset..];
            if !frame.header
                && let Some(literal) = spec
                    .definition
                    .literal_escapes
                    .iter()
                    .filter(|literal| rest.starts_with(literal.as_str()))
                    .max_by_key(|literal| literal.len())
            {
                position.advance(literal);
                continue;
            }
            let escape = if frame.header {
                spec.definition
                    .header
                    .as_ref()
                    .and_then(|h| h.escape.as_ref())
            } else {
                spec.definition.escape.as_ref()
            };
            if let Some(escape) = escape
                && rest.starts_with(escape)
            {
                position.advance(escape);
                if let Some(ch) = source[position.offset..].chars().next() {
                    position.advance(&ch.to_string());
                }
                continue;
            }
            if !frame.header {
                let mut matched = None;
                for boundary in &spec.close_before {
                    budget.spend()?;
                    if (boundary.after_kinds.is_empty()
                        || frame
                            .previous
                            .as_ref()
                            .is_some_and(|p| boundary.after_kinds.contains(&p.kind)))
                        && let Some(found) = boundary.regex.find(rest)
                    {
                        matched = Some(found.end());
                        break;
                    }
                }
                if let Some(length) = matched {
                    let mut end = position;
                    end.advance(&rest[..length]);
                    let region = &mut regions[frame.index];
                    region.span.end = position;
                    region.content_span.end = position;
                    region.lookahead_span = Some(SourceSpan {
                        start: position,
                        end,
                    });
                    region.closed = true;
                    Self::pop_frame(&mut stack, &regions);
                    if stack.is_empty() {
                        return Ok(LexicalRegionScan {
                            length: position.offset - start.offset,
                            regions,
                        });
                    }
                    continue;
                }
            }
            if frame.header {
                let h = spec.definition.header.as_ref().expect("header definition");
                let terminal = frame
                    .terminal_value
                    .as_ref()
                    .or(h.terminal.as_ref())
                    .filter(|s| rest.starts_with(s.as_str()));
                let end = rest.starts_with(&h.end);
                if let Some(terminal) = terminal
                    && (!end || terminal.len() > h.end.len())
                {
                    let before = position;
                    position.advance(terminal);
                    let region = &mut regions[frame.index];
                    region.opener.end = before;
                    region.content_span.start = before;
                    region.header = Some(SourceSpan {
                        start: region.span.start,
                        end: position,
                    });
                    region
                        .header_content_span
                        .as_mut()
                        .expect("header content")
                        .end = before;
                    region.header_closed = true;
                    region.self_closed = true;
                    Self::finish(region, before, position);
                    Self::pop_frame(&mut stack, &regions);
                    if stack.is_empty() {
                        return Ok(LexicalRegionScan {
                            length: position.offset - start.offset,
                            regions,
                        });
                    }
                    continue;
                }
                if end {
                    let before = position;
                    position.advance(&h.end);
                    let region = &mut regions[frame.index];
                    region
                        .header_content_span
                        .as_mut()
                        .expect("header content")
                        .end = before;
                    region.header_closed = true;
                    region.opener.end = position;
                    region.content_span.start = position;
                    region.header = Some(SourceSpan {
                        start: region.span.start,
                        end: position,
                    });
                    stack.last_mut().expect("header frame").header = false;
                    continue;
                }
            } else if let Some(length) = Closing::length(rest, frame, spec) {
                let before = position;
                position.advance(&rest[..length]);
                Self::finish(&mut regions[frame.index], before, position);
                Self::pop_frame(&mut stack, &regions);
                if stack.is_empty() {
                    return Ok(LexicalRegionScan {
                        length: position.offset - start.offset,
                        regions,
                    });
                }
                continue;
            }
            if !frame.header
                && spec
                    .definition
                    .recovery_prefixes
                    .iter()
                    .any(|p| rest.starts_with(p))
            {
                let mut ancestor = None;
                for (index, parent) in stack.iter().enumerate().rev().skip(1) {
                    budget.spend()?;
                    if Closing::length(rest, parent, &self.modes[&parent.mode]).is_some() {
                        ancestor = Some(index);
                        break;
                    }
                    if parent.header || self.modes[&parent.mode].definition.recovery_barrier {
                        break;
                    }
                }
                if let Some(ancestor) = ancestor {
                    while stack.len() > ancestor + 1 {
                        let popped = stack.pop().expect("recovering child");
                        let region = &mut regions[popped.index];
                        region.span.end = position;
                        region.content_span.end = position;
                        region.closed = false;
                    }
                    continue;
                }
            }
            if !frame.header {
                for transition in &frame.counted_children {
                    let Some((prefix, length)) = transition.lengths(rest, budget)? else {
                        continue;
                    };
                    if length == 0 {
                        position.advance(&rest[..prefix]);
                        continue 'scan;
                    }
                    let mode = &transition.definition.region;
                    if !self.accepts(mode, &rest[prefix + length..]) {
                        continue;
                    }
                    if stack.len() >= self.max_depth {
                        return Err(NexoraError::Limit("lexical region depth budget".into()));
                    }
                    budget.node()?;
                    position.advance(&rest[..prefix]);
                    let child_spec = &self.modes[mode].definition;
                    let terminal_value = child_spec
                        .header
                        .as_ref()
                        .and_then(|h| h.terminal.as_ref())
                        .map(|_| transition.closing.clone());
                    let opening = LexicalRegionOpening::literal(
                        position,
                        &rest[prefix..prefix + length],
                        mode,
                        child_spec,
                        regions[frame.index].id.clone(),
                        transition.closing[child_spec.close.len()..].to_owned(),
                    );
                    position = opening.end;
                    let index = regions.len();
                    let next_frame = LexicalRegionFrame {
                        previous: None,
                        returned_kind: child_spec.token_kind.clone(),
                        returned_trivia: child_spec.trivia,
                        mode: mode.clone(),
                        index,
                        header: child_spec.header.is_some(),
                        close_value: opening.close_value,
                        terminal_value,
                        counted_children: Vec::new(),
                    };
                    regions.push(opening.region);
                    stack.push(next_frame);
                    continue 'scan;
                }
            }
            let transitions = if frame.header {
                &spec.header_children
            } else {
                &spec.children
            };
            let mut child = None;
            for transition in transitions {
                budget.spend()?;
                if let Some(found) = transition.regex.find(rest) {
                    if found.end() == 0 {
                        return Err(NexoraError::Configuration(
                            "lexical transition must advance".into(),
                        ));
                    }
                    if !transition.reject && !self.accepts(&transition.region, &rest[found.end()..])
                    {
                        continue;
                    }
                    if transition.reject {
                        return Err(NexoraError::Lexical(format!(
                            "rejected transition in lexical mode {} at byte {}",
                            frame.mode, position.offset
                        )));
                    }
                    child = Some(transition);
                    break;
                }
            }
            if let Some(child) = child {
                if stack.len() >= self.max_depth {
                    return Err(NexoraError::Limit("lexical region depth budget".into()));
                }
                budget.node()?;
                let opening = LexicalRegionOpening::new(
                    source,
                    position,
                    &child.regex,
                    &child.region,
                    &self.modes[&child.region].definition,
                    Some(regions[frame.index].id.clone()),
                    budget,
                )?;
                position = opening.end;
                let index = regions.len();
                regions.push(opening.region);
                stack.push(LexicalRegionFrame {
                    previous: None,
                    returned_kind: self.modes[&child.region].definition.token_kind.clone(),
                    returned_trivia: self.modes[&child.region].definition.trivia,
                    mode: child.region.clone(),
                    index,
                    header: self.modes[&child.region].definition.header.is_some(),
                    close_value: opening.close_value,
                    terminal_value: None,
                    counted_children: ResolvedCountedTransition::resolve(
                        source,
                        &regions[index],
                        &self.modes[&child.region].definition.counted_children,
                        &self.modes,
                        budget,
                    )?,
                });
            } else if !frame.header && !spec.token_rules.is_empty() {
                let previous = frame.previous.as_ref().map(|t| t.context(source));
                let (rule, length) = crate::token_rule_selector::TokenRuleSelector::select(
                    &spec.token_rules,
                    &spec.token_rule_set,
                    rest,
                    position.offset,
                    previous,
                    |mode, suffix| self.accepts(mode, suffix),
                    Some(budget),
                )?;
                if let Some(mode) = &rule.region {
                    if stack.len() >= self.max_depth {
                        return Err(NexoraError::Limit("lexical region depth budget".into()));
                    }
                    budget.node()?;
                    let opening = LexicalRegionOpening::new(
                        source,
                        position,
                        &rule.regex,
                        mode,
                        &self.modes[mode].definition,
                        Some(regions[frame.index].id.clone()),
                        budget,
                    )?;
                    position = opening.end;
                    let index = regions.len();
                    regions.push(opening.region);
                    stack.push(LexicalRegionFrame {
                        previous: None,
                        returned_kind: Some(rule.kind.clone()),
                        returned_trivia: rule.trivia,
                        mode: mode.clone(),
                        index,
                        header: self.modes[mode].definition.header.is_some(),
                        close_value: opening.close_value,
                        terminal_value: None,
                        counted_children: ResolvedCountedTransition::resolve(
                            source,
                            &regions[index],
                            &self.modes[mode].definition.counted_children,
                            &self.modes,
                            budget,
                        )?,
                    });
                } else {
                    let start = position;
                    let text = &rest[..length];
                    let kind = crate::keyword_classifier::KeywordClassifier::classify(
                        &rule.kind,
                        text,
                        frame.previous.as_ref().map(|t| t.context(source)),
                        &self.identifier_kind,
                        &self.keywords,
                        &self.keyword_after_exclusions,
                    );
                    position.advance(text);
                    if !rule.trivia {
                        stack.last_mut().expect("token frame").previous =
                            Some(crate::context_token::ContextToken {
                                kind,
                                span: SourceSpan {
                                    start,
                                    end: position,
                                },
                            });
                    }
                }
            } else {
                let length = rest.chars().next().expect("nonempty suffix").len_utf8();
                position.advance(&rest[..length]);
            }
        }
        let mut descendants_closed = true;
        for frame in stack.into_iter().rev() {
            let region = &mut regions[frame.index];
            region.span.end = position;
            if frame.header {
                region
                    .header_content_span
                    .as_mut()
                    .expect("header content")
                    .end = position;
                region.opener.end = position;
                region.content_span.start = position;
                region.header = Some(SourceSpan {
                    start: region.span.start,
                    end: position,
                });
            }
            region.content_span.end = position;
            region.closed = descendants_closed
                && !frame.header
                && self.modes[&frame.mode].definition.eof_closes;
            descendants_closed = region.closed;
        }
        Ok(LexicalRegionScan {
            length: position.offset - start.offset,
            regions,
        })
    }
}

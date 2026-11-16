use crate::{
    MatchFailure, PatternMatcher,
    compiled_expression_receiver_filter::CompiledExpressionReceiverFilter,
};
use nexora_core::ExpressionKind;
use nexora_language::ExpressionDefinition;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct CompiledExpressionReceiverPolicy {
    pub operators: BTreeMap<String, u8>,
    pub filters: Vec<Vec<CompiledExpressionReceiverFilter>>,
}
impl CompiledExpressionReceiverPolicy {
    pub fn failure(at: usize) -> MatchFailure {
        MatchFailure {
            at,
            message: "invalid expression receiver policy".into(),
            fatal: true,
        }
    }
    pub fn kind_bit(kind: ExpressionKind, at: usize) -> Result<u16, MatchFailure> {
        1u16.checked_shl(kind as u32)
            .ok_or_else(|| Self::failure(at))
    }
    fn kind_mask(
        kinds: &[ExpressionKind],
        matcher: &mut PatternMatcher<'_>,
        at: usize,
    ) -> Result<u16, MatchFailure> {
        if kinds.len() > 11 {
            return Err(Self::failure(at));
        }
        let mut mask = 0;
        for kind in kinds {
            matcher.spend(at)?;
            let bit = Self::kind_bit(*kind, at)?;
            if mask & bit != 0 {
                return Err(Self::failure(at));
            }
            mask |= bit;
        }
        Ok(mask)
    }
    pub fn compile(
        e: &ExpressionDefinition,
        matcher: &mut PatternMatcher<'_>,
        at: usize,
    ) -> Result<Self, MatchFailure> {
        let total: usize = e
            .application
            .iter()
            .map(|a| a.excluded_receivers.len())
            .sum();
        if matcher.grammar.schema_version < 10
            || e.application.len() > 16
            || total > 64
            || e.grouping.len() > 16
            || e.prefix
                .len()
                .saturating_add(e.infix.len())
                .saturating_add(e.member.len())
                .saturating_add(e.postfix.len())
                .saturating_add(e.conditional.len())
                .saturating_add(e.application.len())
                > 128
        {
            return Err(Self::failure(at));
        }
        let mut texts = BTreeSet::new();
        for text in e.node_operator_texts() {
            matcher.spend(at)?;
            if text.is_empty() || text.len() > 128 {
                return Err(Self::failure(at));
            }
            texts.insert(text);
            if texts.len() > 256 {
                return Err(Self::failure(at));
            }
        }
        let mut operators = BTreeMap::new();
        for (index, text) in texts.into_iter().enumerate() {
            matcher.spend(at)?;
            operators.insert(
                text.to_owned(),
                u8::try_from(index).map_err(|_| Self::failure(at))?,
            );
        }
        let mut filters = Vec::with_capacity(e.application.len());
        for application in &e.application {
            matcher.spend(at)?;
            if application.excluded_receivers.len() > 16 {
                return Err(Self::failure(at));
            }
            let mut compiled = Vec::with_capacity(application.excluded_receivers.len());
            for filter in &application.excluded_receivers {
                matcher.spend(at)?;
                if filter.kinds.is_empty()
                    || filter.token.as_ref().is_some_and(|t| {
                        t.text.is_empty()
                            || t.text.len() > 128
                            || t.kind
                                .as_ref()
                                .is_some_and(|k| k.is_empty() || k.len() > 128)
                    })
                {
                    return Err(Self::failure(at));
                }
                let kinds = Self::kind_mask(&filter.kinds, matcher, at)?;
                let operator = filter
                    .operator
                    .as_ref()
                    .map(|text| {
                        matcher.spend(at)?;
                        operators
                            .get(text)
                            .copied()
                            .ok_or_else(|| Self::failure(at))
                    })
                    .transpose()?;
                let descendant_kinds = filter
                    .descendant_kinds
                    .as_ref()
                    .map(|k| Self::kind_mask(k, matcher, at))
                    .transpose()?;
                let descendant_operators = filter
                    .descendant_operators
                    .as_ref()
                    .map(|list| {
                        if list.len() > 256 {
                            return Err(Self::failure(at));
                        }
                        let mut mask = [0u64; 4];
                        for text in list {
                            matcher.spend(at)?;
                            let id =
                                usize::from(*operators.get(text).ok_or_else(|| Self::failure(at))?);
                            let bit = 1u64 << (id % 64);
                            if mask[id / 64] & bit != 0 {
                                return Err(Self::failure(at));
                            }
                            mask[id / 64] |= bit;
                        }
                        Ok(mask)
                    })
                    .transpose()?;
                compiled.push(CompiledExpressionReceiverFilter {
                    kinds,
                    operator,
                    token: filter.token.clone(),
                    descendant_kinds,
                    descendant_operators,
                });
            }
            filters.push(compiled);
        }
        Ok(Self { operators, filters })
    }
}

use crate::{
    ExpressionDefinition, GrammarDefinition, GraphDefinition, LexerDefinition, TokenSelector,
};
use nexora_core::{ExpressionKind, NexoraError, SyntaxRole};
use std::collections::BTreeSet;
pub struct ExpressionValidator;
impl ExpressionValidator {
    fn error() -> NexoraError {
        NexoraError::Configuration(
            "invalid expression table, delimiter or operator contract".into(),
        )
    }
    fn overlap(a: &TokenSelector, b: &TokenSelector) -> bool {
        a.overlaps(b)
    }
    pub fn validate(
        g: &GrammarDefinition,
        l: &LexerDefinition,
        graph: &GraphDefinition,
    ) -> Result<(), NexoraError> {
        if g.productions.iter().any(|p| {
            p.projection.role == SyntaxRole::Expression
                || graph.mappings.get(&p.projection.ast_kind).is_some_and(|k| {
                    k == "Expression"
                        || (k == "Statement" && p.projection.role != SyntaxRole::Container)
                })
        }) {
            return Err(Self::error());
        }
        let kinds: BTreeSet<_> = l
            .tokens
            .iter()
            .filter(|t| !t.trivia)
            .map(|t| t.kind.as_str())
            .chain(std::iter::once("keyword"))
            .collect();
        for c in g.contexts.values() {
            if let Some(e) = &c.expression {
                if ![4, 5, 6, 7, 8, 9, 10].contains(&g.schema_version)
                    || ![4, 5, 6, 7, 8, 9, 10].contains(&graph.schema_version)
                    || e.ast_kind.is_empty()
                    || e.ast_kind.len() > 128
                    || graph
                        .mappings
                        .get(&e.ast_kind)
                        .is_none_or(|k| k != "Expression")
                    || e.identifiers.is_empty()
                    || e.identifiers.len() > 8
                    || e.literals.len() > 8
                {
                    return Err(Self::error());
                }
                let atoms: Vec<_> = e.identifiers.iter().chain(e.literals.iter()).collect();
                if atoms.iter().any(|k| !kinds.contains(k.as_str()))
                    || atoms.iter().collect::<BTreeSet<_>>().len() != atoms.len()
                {
                    return Err(Self::error());
                }
                let valid = |s: &TokenSelector| {
                    !s.text.is_empty()
                        && s.text.len() <= 128
                        && s.kind.as_ref().is_none_or(|k| kinds.contains(k.as_str()))
                };
                crate::expression_form_filter_validator::ExpressionFormFilterValidator::validate(
                    e, g, &valid,
                )?;
                crate::conditional_operator_validator::ConditionalOperatorValidator::validate(
                    e, g, l, &valid,
                )?;
                if e.literal_selectors.len() > 32
                    || (g.schema_version < 5 && !e.literal_selectors.is_empty())
                    || e.literal_selectors.iter().any(|s| !valid(s))
                {
                    return Err(Self::error());
                }
                for (i, selector) in e.literal_selectors.iter().enumerate() {
                    if e.literal_selectors[i + 1..]
                        .iter()
                        .any(|s| Self::overlap(selector, s))
                        || e.prefix
                            .iter()
                            .chain(e.postfix.iter())
                            .chain(e.member.iter())
                            .map(|o| &o.token)
                            .chain(e.infix.iter().map(|o| &o.token))
                            .any(|s| Self::overlap(selector, s))
                        || e.grouping.iter().any(|g| {
                            [&g.open, &g.close].into_iter().any(|text| {
                                Self::overlap(
                                    selector,
                                    &TokenSelector {
                                        text: text.clone(),
                                        kind: g.kind.clone(),
                                    },
                                )
                            })
                        })
                        || e.application.iter().any(|a| {
                            Self::overlap(selector, &a.separator)
                                || [&a.open, &a.close].into_iter().any(|text| {
                                    Self::overlap(
                                        selector,
                                        &TokenSelector {
                                            text: text.clone(),
                                            kind: a.kind.clone(),
                                        },
                                    )
                                })
                        })
                    {
                        return Err(Self::error());
                    }
                }
                let prefix: Vec<_> = e.prefix.iter().map(|o| &o.token).collect();
                let tail: Vec<_> = e
                    .infix
                    .iter()
                    .map(|o| &o.token)
                    .chain(e.postfix.iter().map(|o| &o.token))
                    .chain(e.member.iter().map(|o| &o.token))
                    .chain(e.conditional.iter().map(|o| &o.token))
                    .collect();
                if e.grouping.len() > 16
                    || e.application.len() > 16
                    || e.prefix.len() + tail.len() + e.application.len() + e.conditional.len() > 128
                    || prefix.iter().chain(tail.iter()).any(|s| !valid(s))
                {
                    return Err(Self::error());
                }
                for list in [&prefix, &tail] {
                    for (i, a) in list.iter().enumerate() {
                        if list[i + 1..].iter().any(|b| Self::overlap(a, b)) {
                            return Err(Self::error());
                        }
                    }
                }
                if e.prefix
                    .iter()
                    .chain(e.postfix.iter())
                    .chain(e.member.iter())
                    .any(|o| !(1..=1024).contains(&o.precedence))
                    || e.infix.iter().any(|o| {
                        !(1..=1024).contains(&o.precedence)
                            || !matches!(o.kind, ExpressionKind::Infix | ExpressionKind::Assignment)
                    })
                {
                    return Err(Self::error());
                }
                if e.assignment_targets.len() > 3
                    || e.assignment_targets.iter().any(|k| {
                        !matches!(
                            k,
                            ExpressionKind::Identifier
                                | ExpressionKind::Member
                                | ExpressionKind::Index
                        ) || (*k == ExpressionKind::Index
                            && (g.schema_version < 8
                                || !e
                                    .application
                                    .iter()
                                    .any(|a| a.expression_kind == Some(ExpressionKind::Index))))
                    })
                    || e.assignment_targets
                        .iter()
                        .map(|k| format!("{k:?}"))
                        .collect::<BTreeSet<_>>()
                        .len()
                        != e.assignment_targets.len()
                    || (e.infix.iter().any(|o| o.kind == ExpressionKind::Assignment)
                        && e.assignment_targets.is_empty())
                {
                    return Err(Self::error());
                }
                for (index, operator) in e.infix.iter().enumerate() {
                    if e.infix[index + 1..].iter().any(|other| {
                        other.precedence == operator.precedence
                            && other.associativity != operator.associativity
                    }) {
                        return Err(Self::error());
                    }
                }
                Self::delimiters(e, g, &valid, &prefix, &tail)?;
            }
        }
        Ok(())
    }
    fn delimiters(
        e: &ExpressionDefinition,
        g: &GrammarDefinition,
        valid: &impl Fn(&TokenSelector) -> bool,
        prefix: &[&TokenSelector],
        tail: &[&TokenSelector],
    ) -> Result<(), NexoraError> {
        let mut group_opens = Vec::new();
        let mut app_opens = Vec::new();
        let mut stops = Vec::new();
        let scanner_delimiters: Vec<_> = g
            .groups
            .iter()
            .flat_map(|group| {
                [
                    TokenSelector {
                        text: group.open.clone(),
                        kind: group.kind.clone(),
                    },
                    TokenSelector {
                        text: group.close.clone(),
                        kind: group.kind.clone(),
                    },
                ]
            })
            .collect();
        if prefix.iter().chain(tail.iter()).any(|operator| {
            scanner_delimiters
                .iter()
                .any(|delimiter| Self::overlap(operator, delimiter))
        }) || e.application.iter().any(|application| {
            scanner_delimiters
                .iter()
                .any(|delimiter| Self::overlap(&application.separator, delimiter))
        }) {
            return Err(Self::error());
        }
        for d in &e.grouping {
            if !g
                .groups
                .iter()
                .any(|p| p.open == d.open && p.close == d.close && p.kind == d.kind)
            {
                return Err(Self::error());
            }
            group_opens.push(TokenSelector {
                text: d.open.clone(),
                kind: d.kind.clone(),
            });
            stops.push(TokenSelector {
                text: d.close.clone(),
                kind: d.kind.clone(),
            });
        }
        for a in &e.application {
            if (g.schema_version < 8 && (a.expression_kind.is_some() || a.max_arguments.is_some()))
                || a.expression_kind.is_some_and(|kind| {
                    !matches!(kind, ExpressionKind::Application | ExpressionKind::Index)
                })
                || a.max_arguments.is_some_and(|maximum| {
                    maximum > 1024
                        || (maximum == 0 && a.expression_kind == Some(ExpressionKind::Index))
                })
                || !(1..=1024).contains(&a.precedence)
                || !valid(&a.separator)
                || !g
                    .groups
                    .iter()
                    .any(|p| p.open == a.open && p.close == a.close && p.kind == a.kind)
                || a.separator.text == a.open
                || a.separator.text == a.close
            {
                return Err(Self::error());
            }
            app_opens.push(TokenSelector {
                text: a.open.clone(),
                kind: a.kind.clone(),
            });
            stops.push(TokenSelector {
                text: a.close.clone(),
                kind: a.kind.clone(),
            });
            stops.push(a.separator.clone());
        }
        for list in [&group_opens, &app_opens] {
            for (i, a) in list.iter().enumerate() {
                if list[i + 1..].iter().any(|b| Self::overlap(a, b)) {
                    return Err(Self::error());
                }
            }
        }
        if prefix
            .iter()
            .chain(tail.iter())
            .any(|o| stops.iter().any(|s| Self::overlap(o, s)))
            || prefix
                .iter()
                .any(|o| group_opens.iter().any(|s| Self::overlap(o, s)))
            || tail
                .iter()
                .any(|o| app_opens.iter().any(|s| Self::overlap(o, s)))
        {
            return Err(Self::error());
        }
        Ok(())
    }
}

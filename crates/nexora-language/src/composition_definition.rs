use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionDefinition {
    #[serde(default, skip_serializing_if = "is_false")]
    pub direct_calls: bool,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub header_routes: std::collections::BTreeMap<String, crate::CompositionRoute>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub header_roles: std::collections::BTreeMap<String, nexora_core::RegionCoverageMethod>,
    pub schema_version: u32,
    #[serde(default)]
    pub routes: std::collections::BTreeMap<String, crate::CompositionRoute>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub roles: std::collections::BTreeMap<String, nexora_core::RegionCoverageMethod>,
}
impl CompositionDefinition {
    pub fn validate(&self, lexer: &crate::LexerDefinition) -> Result<(), nexora_core::NexoraError> {
        use nexora_core::RegionCoverageMethod as M;
        let fail = || {
            nexora_core::NexoraError::Configuration(
                "invalid composition roles/routes or isolated scope policy".into(),
            )
        };
        if ![1, 2, 3, 4, 5, 6].contains(&self.schema_version)
            || (self.schema_version < 5 && self.direct_calls)
            || self.routes.len() > 64
            || self.roles.len() > 64
            || self.header_routes.len() > 64
            || self.header_roles.len() > 64
            || (self.schema_version == 1 && (!self.roles.is_empty() || self.routes.is_empty()))
            || (self.schema_version == 2 && self.roles.is_empty())
            || (self.schema_version < 3
                && (!self.header_roles.is_empty() || !self.header_routes.is_empty()))
            || (self.schema_version >= 3 && self.roles.is_empty() && self.header_roles.is_empty())
        {
            return Err(fail());
        }
        for (header, roles, routes) in [
            (false, &self.roles, &self.routes),
            (true, &self.header_roles, &self.header_routes),
        ] {
            for (mode, role) in roles {
                let spec = lexer.regions.get(mode).ok_or_else(fail)?;
                let transitions = if header {
                    &spec.header.as_ref().ok_or_else(fail)?.children
                } else {
                    &spec.children
                };
                if (*role == M::Literal
                    && (!transitions.is_empty() || (!header && !spec.token_rules.is_empty())))
                    || (*role == M::Grammar && !routes.contains_key(mode))
                    || (!header
                        && self.schema_version < 3
                        && *role == M::Literal
                        && spec.header.as_ref().is_some_and(|h| !h.children.is_empty()))
                {
                    return Err(fail());
                }
            }
            for (mode, route) in routes {
                if !lexer.regions.contains_key(mode)
                    || (header && lexer.regions[mode].header.is_none())
                    || (self.schema_version >= 2 && roles.get(mode) != Some(&M::Grammar))
                    || route.lexical_context.as_ref().is_some_and(|name| {
                        self.schema_version < 6
                            || name.is_empty()
                            || name.len() > 64
                            || !name
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                    })
                    || route.context.is_empty()
                    || route.context.len() > 64
                    || route.language.0.is_empty()
                    || route.language.0.len() > 128
                    || !route
                        .language
                        .0
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                    || (self.schema_version < 4 && route.scope.parent_lookup)
                {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

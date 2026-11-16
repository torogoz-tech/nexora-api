use crate::{GrammarDefinition, GraphDefinition, LanguageManifest, LexerDefinition};
use nexora_core::NexoraError;
use serde::de::DeserializeOwned;
use std::{collections::HashSet, path::Path};
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageDefinition {
    pub manifest: LanguageManifest,
    pub lexer: LexerDefinition,
    pub grammar: GrammarDefinition,
    pub graph: GraphDefinition,
}
impl LanguageDefinition {
    pub fn load(path: &Path) -> Result<Self, NexoraError> {
        let value = Self {
            manifest: Self::read(&path.join("language.yaml"))?,
            lexer: Self::read(&path.join("lexer.yaml"))?,
            grammar: Self::read(&path.join("grammar.yaml"))?,
            graph: Self::read(&path.join("graph.yaml"))?,
        };
        value.validate()?;
        Ok(value)
    }
    fn read<T: DeserializeOwned>(path: &Path) -> Result<T, NexoraError> {
        use std::io::Read;
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(NexoraError::InvalidPath(path.display().to_string()));
        }
        let mut text = String::new();
        std::fs::File::open(path)?
            .take(65_537)
            .read_to_string(&mut text)?;
        if text.len() > 65_536 {
            return Err(NexoraError::Limit("YAML > 64 KiB".into()));
        }
        // Value's mapping visitor rejects duplicate keys, including nested maps.
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)
            .map_err(|e| NexoraError::Configuration(e.to_string()))?;
        serde_yaml_ng::from_value(value).map_err(|e| NexoraError::Configuration(e.to_string()))
    }
    pub fn validate(&self) -> Result<(), NexoraError> {
        let fail = |s: &str| NexoraError::Configuration(s.into());
        if self.manifest.schema_version != 1 {
            return Err(fail("only schema_version 1 is supported"));
        }
        if self.manifest.id.0.is_empty()
            || self.manifest.id.0.len() > 128
            || self.manifest.id.0 == "."
            || self.manifest.id.0 == ".."
            || !self
                .manifest
                .id
                .0
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || self.manifest.version.split('.').count() != 3
            || self
                .manifest
                .version
                .split('.')
                .any(|v| v.parse::<u32>().is_err())
        {
            return Err(fail("language id and numeric x.y.z version required"));
        }
        if self.manifest.extensions.len() > 64
            || self.manifest.extensions.iter().any(|e| e.len() > 32)
            || self.manifest.extensions.is_empty()
            || self
                .manifest
                .extensions
                .iter()
                .any(|e| e.is_empty() || !e.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            return Err(fail("extensions must be alphanumeric without dot"));
        }
        crate::LexerContextValidator::validate(&self.lexer)?;
        if let Some(composition) = &self.manifest.composition {
            composition.validate(&self.lexer)?;
            if composition.schema_version >= 5 && self.graph.schema_version < 10 {
                return Err(fail("composition schema5 requires graph10"));
            }
            if composition.direct_calls
                && self
                    .grammar
                    .contexts
                    .values()
                    .filter_map(|c| c.expression.as_ref())
                    .any(|e| !self.graph.expression_bindings.contains_key(&e.ast_kind))
            {
                return Err(fail(
                    "direct calls require binding rules for all expression contexts",
                ));
            }
            if composition.schema_version >= 4 && self.graph.schema_version < 9 {
                return Err(fail("composition schema4 requires graph9"));
            }
        }
        if self.lexer.tokens.is_empty() || self.lexer.tokens.len() > 128 {
            return Err(fail("empty or excessive rules / nesting"));
        }
        let mut kinds = HashSet::new();
        for token in &self.lexer.tokens {
            if token.kind.is_empty()
                || !kinds.insert(token.kind.as_str())
                || token.pattern.len() > 4096
            {
                return Err(fail("empty/duplicate kind or excessive regex"));
            }
            let regex = regex::RegexBuilder::new(&format!("^(?:{})", token.pattern))
                .size_limit(1_000_000)
                .build()
                .map_err(|e| fail(&e.to_string()))?;
            if regex.is_match("") {
                return Err(fail("token regex must not match empty input"));
            }
        }
        if !kinds.contains(self.lexer.identifier_kind.as_str()) {
            return Err(fail("missing identifier token kind"));
        }
        if !self.lexer.regions.is_empty()
            && (self.grammar.schema_version < 4
                || self.graph.schema_version < 7
                || self.lexer.regions.values().any(|r| {
                    self.graph
                        .mappings
                        .get(&r.ast_kind)
                        .is_none_or(|k| k != "LexicalRegion")
                }))
        {
            return Err(fail(
                "lexical regions require hierarchical grammar and region graph mappings",
            ));
        }
        crate::GrammarValidator::validate(&self.grammar, &self.lexer, &self.graph)?;
        crate::GraphValidator::validate(&self.graph, &self.grammar)?;
        let module_references = self.grammar.productions.iter().any(|p| {
            p.projection.role == nexora_core::SyntaxRole::Reference
                && self
                    .graph
                    .mappings
                    .get(&p.projection.ast_kind)
                    .is_some_and(|k| k == "Module")
        });
        if module_references && self.manifest.resolution.is_none() {
            return Err(fail(
                "module reference projections need a resolution strategy",
            ));
        }
        if let Some(resolution) = &self.manifest.resolution {
            crate::ResolutionValidator::validate(resolution)?;
        }
        Ok(())
    }
}

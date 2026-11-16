use nexora_core::NexoraError;
use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
pub struct AnalysisPlan {
    pub lexer: Lexer,
    pub parser: Parser,
    pub resolution: Option<crate::ResolutionPlan>,
}
impl AnalysisPlan {
    pub fn compile(language: &LanguageDefinition) -> Result<Self, NexoraError> {
        language.validate()?;
        Ok(Self {
            lexer: Lexer::new(&language.lexer)?,
            parser: Parser::new(language.grammar.clone()),
            resolution: language
                .manifest
                .resolution
                .as_ref()
                .map(|d| crate::ResolutionPlan::compile(d, &language.manifest.extensions))
                .transpose()?,
        })
    }
}

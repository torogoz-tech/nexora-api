use crate::token_context::TokenContext;
pub(crate) struct KeywordClassifier;
impl KeywordClassifier {
    pub fn classify(
        kind: &str,
        text: &str,
        previous: Option<TokenContext<'_>>,
        identifier: &str,
        keywords: &[String],
        exclusions: &[nexora_language::TokenSelector],
    ) -> String {
        if kind == identifier
            && keywords.iter().any(|k| k == text)
            && previous
                .as_ref()
                .is_none_or(|p| !exclusions.iter().any(|s| p.matches(s)))
        {
            "keyword".into()
        } else {
            kind.into()
        }
    }
}

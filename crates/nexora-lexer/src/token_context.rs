pub(crate) struct TokenContext<'a> {
    pub kind: &'a str,
    pub text: &'a str,
}
impl TokenContext<'_> {
    pub fn matches(&self, selector: &nexora_language::TokenSelector) -> bool {
        selector.text == self.text
            && selector
                .kind
                .as_deref()
                .is_none_or(|kind| kind == self.kind)
    }
}

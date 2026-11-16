pub(crate) struct CompiledExpressionReceiverFilter {
    pub kinds: u16,
    pub operator: Option<u8>,
    pub token: Option<nexora_language::TokenSelector>,
    pub descendant_kinds: Option<u16>,
    pub descendant_operators: Option<[u64; 4]>,
}

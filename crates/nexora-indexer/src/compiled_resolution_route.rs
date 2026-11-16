#[derive(Clone)]
pub(crate) struct CompiledResolutionRoute {
    pub route: nexora_language::ResolutionRoute,
    pub accept: regex::Regex,
}

use std::collections::BTreeSet;
#[derive(Default)]
pub(crate) struct PatternProperties {
    pub nullable: bool,
    pub required: BTreeSet<String>,
    pub possible: BTreeSet<String>,
}

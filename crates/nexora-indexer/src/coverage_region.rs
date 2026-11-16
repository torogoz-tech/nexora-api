pub(crate) struct CoverageRegion {
    pub cst_index: usize,
    pub content_index: usize,
    pub header_index: Option<usize>,
    pub header_role: Option<nexora_core::RegionCoverageMethod>,
    pub header_link: Option<usize>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub role: Option<nexora_core::RegionCoverageMethod>,
    pub link: Option<usize>,
}
impl CoverageRegion {
    pub fn component(
        &self,
        component: nexora_core::LexicalRegionComponent,
    ) -> Option<(
        usize,
        Option<nexora_core::RegionCoverageMethod>,
        Option<usize>,
    )> {
        match component {
            nexora_core::LexicalRegionComponent::Content => {
                Some((self.content_index, self.role, self.link))
            }
            nexora_core::LexicalRegionComponent::Header => self
                .header_index
                .map(|index| (index, self.header_role, self.header_link)),
        }
    }
}

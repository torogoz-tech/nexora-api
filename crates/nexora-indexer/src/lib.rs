mod analysis_result;
pub use analysis_result::*;
mod source_reader;
pub use source_reader::*;
mod indexer;
pub use indexer::*;
mod repository_walker;
pub use nexora_storage::MemoryStore;
pub use repository_walker::*;
mod module_specifier;
pub use module_specifier::*;
mod module_resolution;
pub use module_resolution::*;
mod module_resolver;
pub use module_resolver::*;
mod compiled_resolution_route;
mod resolution_plan;
pub use resolution_plan::*;
mod analysis_plan;
pub use analysis_plan::*;
mod candidate_renderer;
pub use candidate_renderer::*;

pub const ANALYSIS_ENGINE_VERSION: &str =
    "nexora-analysis/0.35.2-composition7-lexer14-grammar10-graph10";
mod stage_timings;
pub use stage_timings::*;
mod measured_analysis;
pub use measured_analysis::*;
mod measured_snapshot;
pub use measured_snapshot::*;

mod index_scope;

mod cached_file;

mod index_cache;
pub use index_cache::*;

mod reuse_statistics;
pub use reuse_statistics::*;

mod incremental_snapshot;
pub use incremental_snapshot::*;

mod composition_limits;
pub use composition_limits::*;
mod composition_budget;
mod syntax_unit;
pub use syntax_unit::*;
mod region_interpretation;
pub use region_interpretation::*;
mod composition_result;
pub use composition_result::*;
mod compiled_composition_profile;
mod composition_analyzer;
mod composition_task;
pub use composition_analyzer::*;

mod composition_graph_projector;
mod unit_graph_namespace;

mod index_analysis_plan;

mod composition_binding_request;
mod composition_binding_resolver;
mod composition_call_request;
mod composition_call_resolver;
mod composition_call_uncertainty;
mod composition_coverage_evaluator;
mod coverage_region;
mod file_cache_key;
pub use file_cache_key::*;
mod cache_fragment;
pub use cache_fragment::*;
mod external_file_cache;
pub use external_file_cache::*;

mod cache_fragment_digest;

mod analysis_profile_compiler;
pub use analysis_profile_compiler::*;

mod analysis_profile_candidate;
pub use analysis_profile_candidate::*;
mod multi_profile_indexer;
pub use multi_profile_indexer::*;
mod multi_profile_index_result;
pub use multi_profile_index_result::*;

mod source_read_outcome;
pub use source_read_outcome::*;
mod source_text_decoder;

pub use source_text_decoder::SourceTextDecoder;

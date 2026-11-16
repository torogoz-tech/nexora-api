use super::RepositoryAnalysisProfileMcpTools;
use serde_json::json;

#[test]
fn large_selection_reports_never_overrun_either_mcp_envelope() {
    for structured in [true, false] {
        let result = RepositoryAnalysisProfileMcpTools::output(
            Ok(json!({"last_selection":{"uncovered_files":["\"".repeat(600_000)]}})),
            structured,
        )
        .unwrap();
        assert_eq!(result["isError"], true);
        assert!(serde_json::to_vec(&result).unwrap().len() < 1_000_000);
    }
}
#[test]
fn ordinary_provenance_is_preserved() {
    let report = json!({"mode":"automatic","last_selection":{"policy":"automatic-latest/v1","profiles":[],"uncovered_files":["README.md"]}});
    let result = RepositoryAnalysisProfileMcpTools::output(Ok(report.clone()), true).unwrap();
    assert_eq!(result["structuredContent"], report);
    assert_eq!(result["isError"], false);
}

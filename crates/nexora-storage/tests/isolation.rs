use nexora_core::{EdgeId, NodeId, RepositoryId, SnapshotId, SourcePosition, SourceSpan, TenantId};
use nexora_graph::{CodeEdge, CodeGraph, EdgeKind, Evidence, ResolutionState};
use nexora_storage::{MemoryStore, Snapshot, SnapshotStore};
fn snapshot(tenant: &str, repo: &str) -> Snapshot {
    Snapshot {
        tenant_id: TenantId(tenant.into()),
        repository_id: RepositoryId(repo.into()),
        id: SnapshotId("v1".into()),
        graph: CodeGraph::default(),
        sources: Default::default(),
        diagnostics: vec![],
        rule_version: "0.1.0".into(),
        analysis_engine: None,
        analysis_configuration: None,
        source_coverage: vec![],
    }
}
#[test]
fn publication_is_immutable_and_isolated() {
    let mut store = MemoryStore::default();
    store.publish(snapshot("a", "one")).unwrap();
    store.publish(snapshot("b", "one")).unwrap();
    store.publish(snapshot("a", "two")).unwrap();
    assert!(
        store
            .get(
                &TenantId("x".into()),
                &RepositoryId("one".into()),
                &SnapshotId("v1".into())
            )
            .is_none()
    );
    assert!(store.publish(snapshot("a", "one")).is_err());
    assert!(
        store
            .get(
                &TenantId("a".into()),
                &RepositoryId("two".into()),
                &SnapshotId("v1".into())
            )
            .is_some()
    );
}
#[test]
fn invalid_graph_never_publishes() {
    let mut store = MemoryStore::default();
    let mut bad = snapshot("a", "one");
    bad.graph.edges.push(CodeEdge {
        call_binding: None,
        scope_binding: None,
        resolution_reason: None,
        label: None,
        id: EdgeId("e".into()),
        source: NodeId("absent".into()),
        target: NodeId("also-absent".into()),
        kind: EdgeKind::Calls,
        evidence: Evidence {
            syntax_unit: None,
            path: "a.js".into(),
            span: SourceSpan {
                start: SourcePosition::default(),
                end: SourcePosition::default(),
            },
            ast_id: NodeId("ast".into()),
            rule_version: "0.1.0".into(),
        },
        resolution: ResolutionState::Unresolved,
    });
    assert!(store.publish(bad).is_err());
    assert!(
        store
            .get(
                &TenantId("a".into()),
                &RepositoryId("one".into()),
                &SnapshotId("v1".into())
            )
            .is_none()
    );
}

use nexora_governance::*;
use std::path::Path;
fn policy() -> Policy {
    GovernanceIo::yaml(
        &Path::new("../../policies/examples/naming.policy.yaml")
            .canonicalize()
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn compile_and_registry_reject_invalid_versions_duplicates_and_references() {
    let mut registry = PolicyRegistry::default();
    registry.register(policy()).unwrap();
    assert!(registry.register(policy()).is_err());
    let mut p = policy();
    p.metadata.version = PolicyVersion("01.0.0".into());
    assert!(PolicyCompiler::compile(p).is_err());
    let mut p = policy();
    p.spec.requires.push(PolicyReference {
        id: "missing/policy".into(),
        version: PolicyVersion("1.0.0".into()),
    });
    let mut registry = PolicyRegistry::default();
    registry.register(p).unwrap();
    assert!(registry.validate_references().is_err());
}
#[test]
fn unknown_predicates_and_incompatible_operators_are_configuration_errors() {
    for (attribute, operator, value) in [
        ("secretAttribute", "equals", Some("x")),
        ("name", "exec", Some("x")),
        ("kind", "matches", Some(".*")),
        ("name", "exists", Some("x")),
    ] {
        let mut p = policy();
        p.spec.assert =
            PolicyAssertion::Predicate(PolicyPredicate::Attribute(AttributePredicate {
                attribute: attribute.into(),
                operator: operator.into(),
                value: value.map(String::from),
            }));
        assert!(PolicyCompiler::compile(p).is_err());
    }
}
#[test]
fn unknown_relation_is_rejected_but_known_missing_capability_compiles() {
    let mut p = policy();
    p.spec.assert = PolicyAssertion::Predicate(PolicyPredicate::Relation(RelationPredicate {
        relation: "MAGIC_QUERY".into(),
        target: Default::default(),
        traversal: Default::default(),
    }));
    assert!(PolicyCompiler::compile(p).is_err());
    let mut p = policy();
    p.spec.assert = PolicyAssertion::Predicate(PolicyPredicate::Relation(RelationPredicate {
        relation: "CALLS".into(),
        target: Default::default(),
        traversal: Default::default(),
    }));
    assert!(PolicyCompiler::compile(p).is_ok());
}

#[test]
fn blank_semantic_names_and_scope_strings_are_rejected() {
    for field in ["inherits", "implements"] {
        let mut p = policy();
        let mut filter = PolicyFilter::default();
        if field == "inherits" {
            filter.inherits = Some("   ".into())
        } else {
            filter.implements = Some("\n".into())
        }
        p.spec.target.filter = Some(filter);
        assert!(PolicyCompiler::compile(p).is_err(), "{field}");
    }
    let mut p = policy();
    p.spec.scope.organization = Some(" ".into());
    assert!(PolicyCompiler::compile(p).is_err());
    let mut p = policy();
    p.spec.scope.tenants = vec![" ".into()];
    assert!(PolicyCompiler::compile(p).is_err());
    let mut p = policy();
    p.spec.assert = PolicyAssertion::Predicate(PolicyPredicate::Relation(RelationPredicate {
        relation: "CALLS".into(),
        target: PolicyRelationTarget {
            implements: Some(" ".into()),
            ..Default::default()
        },
        traversal: Default::default(),
    }));
    assert!(PolicyCompiler::compile(p).is_err());
}

#[test]
fn contradictory_attribute_conditions_are_rejected() {
    let mut p = policy();
    p.spec.assert=serde_yaml_ng::from_str("all:\n  - attribute: name\n    operator: equals\n    value: A\n  - attribute: name\n    operator: equals\n    value: B\n").unwrap();
    assert!(PolicyCompiler::compile(p).is_err());
}

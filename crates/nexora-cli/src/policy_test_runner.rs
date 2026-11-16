use crate::{policy_test_result::PolicyTestResult, policy_test_suite::PolicyTestSuite};
use nexora_core::TenantId;
use nexora_governance::*;
use nexora_indexer::MemoryStore;
use nexora_language::LanguageRegistry;
use std::path::{Component, Path};
pub struct PolicyTestRunner;
impl PolicyTestRunner {
    pub fn run(
        root: &Path,
        catalog: &LanguageRegistry,
        explicit: Option<&str>,
    ) -> Result<Vec<PolicyTestResult>, GovernanceError> {
        let root = GovernanceIo::path(root)?;
        let registry = PolicyRegistry::load(&root)?;
        let mut results = Vec::new();
        for path in GovernanceIo::discover(&root, ".tests.yaml")? {
            let suite: PolicyTestSuite = GovernanceIo::yaml(&path)?;
            if suite.api_version != "nexora.dev/v1alpha1"
                || suite.kind != "PolicyTestSuite"
                || suite.cases.is_empty()
            {
                return Err(GovernanceError::Configuration(
                    "invalid PolicyTestSuite".into(),
                ));
            }
            let parent = path
                .parent()
                .ok_or_else(|| GovernanceError::Configuration("fixture parent".into()))?;
            for case in suite.cases {
                if results.len() >= 256 {
                    return Err(GovernanceError::Limit("policy test cases > 256".into()));
                }
                let source = Path::new(&case.source);
                if source.is_absolute()
                    || source
                        .components()
                        .any(|c| matches!(c, Component::ParentDir))
                {
                    return Err(GovernanceError::Scope(
                        "fixture source must remain inside policy directory".into(),
                    ));
                }
                let source = GovernanceIo::path(&parent.join(source))?;
                if !source.starts_with(&root) {
                    return Err(GovernanceError::Scope(
                        "fixture source escapes policy directory".into(),
                    ));
                }
                if explicit.is_some()
                    && case
                        .language
                        .as_deref()
                        .is_some_and(|id| Some(id) != explicit)
                {
                    return Err(GovernanceError::Scope(
                        "fixture language conflicts with selected scope".into(),
                    ));
                }
                let language = crate::language_selection::LanguageSelection::path(
                    catalog,
                    case.language.as_deref().or(explicit),
                    &source,
                )
                .map_err(|e| GovernanceError::Configuration(e.to_string()))?;
                let policy = registry.get(&case.policy).ok_or_else(|| {
                    GovernanceError::Configuration(format!(
                        "fixture policy {} not found",
                        case.policy
                    ))
                })?;
                let tenant = TenantId("local".into());
                let snapshot = crate::language_selection::LanguageSelection::indexer(
                    catalog,
                    language.clone(),
                )
                .map_err(|e| GovernanceError::Configuration(e.to_string()))?
                .index(&source, tenant.clone(), &mut MemoryStore::default())
                .map_err(|e| GovernanceError::Configuration(e.to_string()))?;
                let mut context = EvaluationContext::new(
                    &snapshot,
                    &tenant,
                    &snapshot.repository_id,
                    &language.manifest.id.0,
                    case.organization.as_deref(),
                )?;
                context.coverage = AnalysisCoverage::indexed(&snapshot);
                let evaluation = PolicyEvaluator::evaluate(&context, std::slice::from_ref(policy))?;
                results.push(PolicyTestResult {
                    name: case.name,
                    policy: case.policy,
                    expected: case.expected,
                    actual: evaluation.status,
                    passed: case.expected == evaluation.status,
                    evaluation_id: evaluation.id,
                });
            }
        }
        if results.is_empty() {
            return Err(GovernanceError::Configuration(
                "no .tests.yaml cases found".into(),
            ));
        }
        Ok(results)
    }
}

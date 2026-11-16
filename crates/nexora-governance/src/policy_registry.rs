use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Default)]
pub struct PolicyRegistry {
    policies: BTreeMap<String, CompiledPolicy>,
}
impl PolicyRegistry {
    pub fn load(root: &Path) -> Result<Self, GovernanceError> {
        let mut registry = Self::default();
        for path in GovernanceIo::discover(root, ".yaml")? {
            if path.to_string_lossy().ends_with(".tests.yaml") {
                continue;
            }
            registry.register(GovernanceIo::yaml(&path)?)?;
        }
        if registry.policies.is_empty() {
            return Err(GovernanceError::Configuration("no policies found".into()));
        }
        registry.validate_references()?;
        Ok(registry)
    }
    pub fn register(&mut self, policy: Policy) -> Result<(), GovernanceError> {
        if self.policies.len() >= 128 {
            return Err(GovernanceError::Limit("policies > 128".into()));
        }
        let policy = PolicyCompiler::compile(policy)?;
        let id = policy.policy.metadata.qualified_id();
        if self.policies.contains_key(&id) {
            return Err(GovernanceError::Configuration(format!(
                "duplicate policy {id}"
            )));
        }
        self.policies.insert(id, policy);
        Ok(())
    }
    pub fn policies(&self) -> impl Iterator<Item = &CompiledPolicy> {
        self.policies.values()
    }
    pub fn get(&self, id: &str) -> Option<&CompiledPolicy> {
        self.policies.get(id)
    }
    pub fn validate_references(&self) -> Result<(), GovernanceError> {
        for id in self.policies.keys() {
            self.visit(id, &mut BTreeSet::new(), &mut BTreeSet::new())?;
        }
        Ok(())
    }
    fn visit(
        &self,
        id: &str,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<(), GovernanceError> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.into()) {
            return Err(GovernanceError::Configuration(format!(
                "cyclic policy reference {id}"
            )));
        }
        let p = self.policies.get(id).ok_or_else(|| {
            GovernanceError::Configuration(format!("missing policy reference {id}"))
        })?;
        for reference in &p.policy.spec.requires {
            reference.version.validate()?;
            let target = self.policies.get(&reference.id).ok_or_else(|| {
                GovernanceError::Configuration(format!("missing policy reference {}", reference.id))
            })?;
            if target.policy.metadata.version != reference.version {
                return Err(GovernanceError::Configuration(format!(
                    "policy reference version mismatch {}",
                    reference.id
                )));
            }
            self.visit(&reference.id, active, done)?;
        }
        active.remove(id);
        done.insert(id.into());
        Ok(())
    }
}

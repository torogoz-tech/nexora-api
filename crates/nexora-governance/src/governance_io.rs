use crate::GovernanceError;
use serde::de::DeserializeOwned;
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};
pub struct GovernanceIo;
impl GovernanceIo {
    pub fn path(path: &Path) -> Result<PathBuf, GovernanceError> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut current = PathBuf::new();
        for component in absolute.components() {
            if matches!(component, Component::ParentDir) {
                return Err(GovernanceError::Scope("parent traversal rejected".into()));
            }
            current.push(component.as_os_str());
            if std::fs::symlink_metadata(&current)?
                .file_type()
                .is_symlink()
            {
                return Err(GovernanceError::Scope("symbolic link rejected".into()));
            }
        }
        Ok(absolute.canonicalize()?)
    }
    pub fn read(path: &Path, limit: u64) -> Result<String, GovernanceError> {
        let path = Self::path(path)?;
        if !std::fs::metadata(&path)?.is_file() {
            return Err(GovernanceError::Configuration(
                "regular file required".into(),
            ));
        }
        let mut text = String::new();
        std::fs::File::open(path)?
            .take(limit + 1)
            .read_to_string(&mut text)?;
        if text.len() as u64 > limit {
            return Err(GovernanceError::Limit("file size".into()));
        }
        Ok(text)
    }
    pub fn yaml<T: DeserializeOwned>(path: &Path) -> Result<T, GovernanceError> {
        let text = Self::read(path, 65_536)?;
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)
            .map_err(|e| GovernanceError::Configuration(format!("{}: {e}", path.display())))?;
        let mut pending = vec![(&value, 0)];
        let mut nodes = 0;
        while let Some((value, depth)) = pending.pop() {
            nodes += 1;
            if depth > 24 || nodes > 2048 {
                return Err(GovernanceError::Limit("YAML depth/nodes".into()));
            }
            match value {
                serde_yaml_ng::Value::Mapping(m) => {
                    for (k, v) in m {
                        pending.push((k, depth + 1));
                        pending.push((v, depth + 1));
                    }
                }
                serde_yaml_ng::Value::Sequence(s) => {
                    for v in s {
                        pending.push((v, depth + 1));
                    }
                }
                serde_yaml_ng::Value::Null => {
                    return Err(GovernanceError::Configuration(
                        "null is not a DSL value; omit optional fields".into(),
                    ));
                }
                serde_yaml_ng::Value::Tagged(_) => {
                    return Err(GovernanceError::Configuration(
                        "YAML tags are not permitted".into(),
                    ));
                }
                _ => {}
            }
        }
        serde_yaml_ng::from_value(value)
            .map_err(|e| GovernanceError::Configuration(format!("{}: {e}", path.display())))
    }
    pub fn discover(root: &Path, suffix: &str) -> Result<Vec<PathBuf>, GovernanceError> {
        let root = Self::path(root)?;
        if root.is_file() {
            return Ok(vec![root]);
        }
        let mut result = Vec::new();
        let mut pending = vec![(root, 0)];
        let mut entries = 0;
        while let Some((directory, depth)) = pending.pop() {
            if depth > 16 {
                return Err(GovernanceError::Limit("policy directory depth".into()));
            }
            for entry in std::fs::read_dir(directory)? {
                entries += 1;
                if entries > 2000 {
                    return Err(GovernanceError::Limit("policy directory entries".into()));
                }
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    return Err(GovernanceError::Scope("policy symlink rejected".into()));
                }
                if kind.is_dir() {
                    pending.push((entry.path(), depth + 1));
                } else if kind.is_file() && entry.file_name().to_string_lossy().ends_with(suffix) {
                    result.push(entry.path());
                }
            }
        }
        result.sort();
        Ok(result)
    }
}

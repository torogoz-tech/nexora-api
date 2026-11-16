use crate::{GitEntryKind, GitObjectFormat, GitTreeEntry, SourceError, SourceLimits};
use std::collections::BTreeMap;

/// Reads `ls-tree -rlz --full-tree` metadata without evaluating repository attributes.
pub struct GitTreeDecoder;
impl GitTreeDecoder {
    pub fn decode(
        bytes: &[u8],
        format: GitObjectFormat,
        limits: &SourceLimits,
    ) -> Result<Vec<GitTreeEntry>, SourceError> {
        limits.validate()?;
        if bytes.len() > limits.max_tree_bytes {
            return Err(SourceError::LimitExceeded);
        }
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        let bytes = bytes.strip_suffix(&[0]).ok_or(SourceError::MalformedTree)?;
        let mut entries = Vec::new();
        // Store each component once rather than every complete prefix of a deep path.
        let mut nodes: BTreeMap<(usize, String), (usize, String, bool)> = BTreeMap::new();
        let mut total = 0u64;
        for record in bytes.split(|byte| *byte == 0) {
            if entries.len() >= limits.max_entries {
                return Err(SourceError::LimitExceeded);
            }
            let separator = record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or(SourceError::MalformedTree)?;
            let header = std::str::from_utf8(&record[..separator])
                .map_err(|_| SourceError::MalformedTree)?;
            let fields: Vec<_> = header.split_ascii_whitespace().collect();
            if fields.len() != 4 {
                return Err(SourceError::MalformedTree);
            }
            let kind = match (fields[0], fields[1]) {
                ("100644", "blob") => GitEntryKind::File,
                ("100755", "blob") => GitEntryKind::ExecutableFile,
                ("120000", "blob") => GitEntryKind::SymbolicLink,
                ("160000", "commit") => GitEntryKind::Submodule,
                _ => return Err(SourceError::MalformedTree),
            };
            let oid = fields[2];
            if oid.len() != format.oid_length()
                || !oid
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(SourceError::MalformedTree);
            }
            let size = if kind == GitEntryKind::Submodule {
                if fields[3] != "-" {
                    return Err(SourceError::MalformedTree);
                }
                None
            } else {
                if !fields[3].bytes().all(|b| b.is_ascii_digit()) {
                    return Err(SourceError::MalformedTree);
                }
                let size = fields[3]
                    .parse::<u64>()
                    .map_err(|_| SourceError::MalformedTree)?;
                if size > limits.max_file_bytes {
                    return Err(SourceError::LimitExceeded);
                }
                total = total.checked_add(size).ok_or(SourceError::LimitExceeded)?;
                if total > limits.max_total_bytes {
                    return Err(SourceError::LimitExceeded);
                }
                Some(size)
            };
            let path = std::str::from_utf8(&record[separator + 1..])
                .map_err(|_| SourceError::UnsafePath)?;
            if path.is_empty()
                || path.len() > limits.max_path_bytes
                || path.contains(['\\', ':'])
                || path.chars().any(char::is_control)
            {
                return Err(SourceError::UnsafePath);
            }
            let parts: Vec<_> = path.split('/').collect();
            if parts.len() > limits.max_path_depth {
                return Err(SourceError::LimitExceeded);
            }
            let mut parent = 0;
            for (index, part) in parts.iter().enumerate() {
                if part.is_empty()
                    || matches!(*part, "." | "..")
                    || part.len() > 255
                    || part.eq_ignore_ascii_case(".git")
                {
                    return Err(SourceError::UnsafePath);
                }
                let leaf = index + 1 == parts.len();
                let key = (parent, part.to_lowercase());
                if let Some((id, original, was_leaf)) = nodes.get(&key) {
                    if original != part || *was_leaf || leaf {
                        return Err(SourceError::UnsafePath);
                    }
                    parent = *id;
                } else {
                    if nodes.len() >= limits.max_path_nodes {
                        return Err(SourceError::LimitExceeded);
                    }
                    let id = nodes.len() + 1;
                    nodes.insert(key, (id, (*part).to_owned(), leaf));
                    parent = id;
                }
            }
            entries.push(GitTreeEntry {
                path: path.into(),
                object_id: oid.into(),
                kind,
                size,
            });
        }
        Ok(entries)
    }
}

use crate::{GitBlobBatch, GitEntryKind, GitObjectFormat, GitTreeEntry, SourceError, SourceLimits};
pub struct GitBatchDecoder;
impl GitBatchDecoder {
    pub(crate) fn output_limit(
        entries: &[GitTreeEntry],
        format: GitObjectFormat,
        limits: &SourceLimits,
    ) -> Result<usize, SourceError> {
        limits.validate()?;
        if entries.len() > limits.max_entries {
            return Err(SourceError::LimitExceeded);
        }
        let mut total = 0u64;
        for entry in entries {
            format.validate_oid(&entry.object_id)?;
            if entry.kind == GitEntryKind::Submodule {
                return Err(SourceError::InvalidObject);
            }
            let size = entry.size.ok_or(SourceError::InvalidObject)?;
            if size > limits.max_file_bytes {
                return Err(SourceError::LimitExceeded);
            }
            total = total.checked_add(size).ok_or(SourceError::LimitExceeded)?;
            if total > limits.max_total_bytes {
                return Err(SourceError::LimitExceeded);
            }
        }
        let headers = entries
            .len()
            .checked_mul(format.oid_length() + 32)
            .ok_or(SourceError::LimitExceeded)?;
        usize::try_from(total)
            .ok()
            .and_then(|bytes| bytes.checked_add(headers))
            .ok_or(SourceError::LimitExceeded)
    }
    pub fn decode(
        bytes: Vec<u8>,
        entries: &[GitTreeEntry],
        format: GitObjectFormat,
        limits: &SourceLimits,
    ) -> Result<GitBlobBatch, SourceError> {
        if bytes.len() > Self::output_limit(entries, format, limits)? {
            return Err(SourceError::LimitExceeded);
        }
        let mut ranges = Vec::with_capacity(entries.len());
        let mut cursor = 0;
        for entry in entries {
            let remaining = bytes.get(cursor..).ok_or(SourceError::InvalidObject)?;
            let header_length = remaining
                .iter()
                .take(format.oid_length() + 32)
                .position(|byte| *byte == 0)
                .ok_or(SourceError::InvalidObject)?;
            let header = std::str::from_utf8(&remaining[..header_length])
                .map_err(|_| SourceError::InvalidObject)?;
            let fields = header.split_ascii_whitespace().collect::<Vec<_>>();
            if fields.len() != 3
                || fields[0] != entry.object_id
                || fields[1] != "blob"
                || !fields[2].bytes().all(|b| b.is_ascii_digit())
            {
                return Err(SourceError::InvalidObject);
            }
            let size = fields[2]
                .parse::<u64>()
                .map_err(|_| SourceError::InvalidObject)?;
            if Some(size) != entry.size {
                return Err(SourceError::InvalidObject);
            }
            cursor = cursor
                .checked_add(header_length + 1)
                .ok_or(SourceError::LimitExceeded)?;
            let end = cursor
                .checked_add(usize::try_from(size).map_err(|_| SourceError::LimitExceeded)?)
                .ok_or(SourceError::LimitExceeded)?;
            if bytes.get(end) != Some(&0) {
                return Err(SourceError::InvalidObject);
            }
            ranges.push(cursor..end);
            cursor = end + 1;
        }
        if cursor != bytes.len() {
            return Err(SourceError::InvalidObject);
        }
        Ok(GitBlobBatch { bytes, ranges })
    }
}

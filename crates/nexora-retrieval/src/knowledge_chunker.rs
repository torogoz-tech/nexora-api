use crate::{KnowledgeChunk, KnowledgeKind};
use sha2::{Digest, Sha256};

pub struct KnowledgeChunker {
    max_chars: usize,
    overlap_chars: usize,
}

impl KnowledgeChunker {
    pub fn new(max_chars: usize, overlap_chars: usize) -> Self {
        assert!(max_chars > 0);
        assert!(overlap_chars < max_chars);
        Self {
            max_chars,
            overlap_chars,
        }
    }

    pub fn chunk(
        &self,
        _kind: KnowledgeKind,
        knowledge_id: &str,
        revision: i64,
        title: &str,
        content: &str,
    ) -> Vec<KnowledgeChunk> {
        let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            return Vec::new();
        }
        let mut chunks = Vec::new();
        let mut start = 0;
        let chars: Vec<char> = normalized.chars().collect();
        while start < chars.len() {
            let end = (start + self.max_chars).min(chars.len());
            let text: String = chars[start..end].iter().collect();
            let content_hash = Self::hash(&text);
            let id = Self::hash(&format!(
                "{knowledge_id}:{revision}:{}:{text}",
                chunks.len()
            ));
            chunks.push(KnowledgeChunk {
                id,
                knowledge_id: knowledge_id.to_owned(),
                revision,
                sequence: chunks.len(),
                title: title.to_owned(),
                content: text,
                content_hash,
            });
            if end == chars.len() {
                break;
            }
            start = end.saturating_sub(self.overlap_chars);
        }
        chunks
    }

    fn hash(content: &str) -> String {
        let mut digest = Sha256::new();
        digest.update(content.as_bytes());
        format!("{:x}", digest.finalize())
    }
}

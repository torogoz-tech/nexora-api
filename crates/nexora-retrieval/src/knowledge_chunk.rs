#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeChunk {
    pub id: String,
    pub knowledge_id: String,
    pub revision: i64,
    pub sequence: usize,
    pub title: String,
    pub content: String,
    pub content_hash: String,
}

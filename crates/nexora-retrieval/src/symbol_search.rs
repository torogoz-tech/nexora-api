use nexora_graph::CodeNode;
pub trait SymbolSearch {
    fn find_symbol(&self, name: &str, limit: usize) -> Vec<&CodeNode>;
}

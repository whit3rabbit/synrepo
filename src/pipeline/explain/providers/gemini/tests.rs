use super::*;
use crate::core::ids::SymbolNodeId;

#[test]
fn new_constructs_without_panicking() {
    let gen =
        GeminiCommentaryGenerator::new("fake-key".to_string(), "test-model".to_string(), 5000);
    let node = NodeId::Symbol(SymbolNodeId(1));
    // This will fail (no API key) but should not panic.
    let _ = gen.generate(node, "context");
}

#[test]
fn oversized_context_skips_generation() {
    let context = "x".repeat(50_000);
    let gen =
        GeminiCommentaryGenerator::new("fake-key".to_string(), "test-model".to_string(), 5000);
    let node = NodeId::Symbol(SymbolNodeId(1));
    let entry = gen.generate(node, &context).unwrap();
    assert!(entry.is_none(), "oversized context must skip generation");
}

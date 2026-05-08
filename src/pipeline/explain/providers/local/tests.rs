use super::*;
use crate::core::ids::SymbolNodeId;

#[test]
fn new_constructs_without_panicking() {
    let gen = LocalCommentaryGenerator::new("test-model".to_string(), 5000);
    let node = NodeId::Symbol(SymbolNodeId(1));
    // This will fail (no local server) but should not panic.
    let _ = gen.generate(node, "context");
}

#[test]
fn oversized_context_skips_generation() {
    let context = "x".repeat(50_000);
    let gen = LocalCommentaryGenerator::new("test-model".to_string(), 5000);
    let node = NodeId::Symbol(SymbolNodeId(1));
    let entry = gen.generate(node, &context).unwrap();
    assert!(entry.is_none(), "oversized context must skip generation");
}

#[test]
fn endpoint_parsing() {
    let gen = LocalCommentaryGenerator::new("llama3".to_string(), 5000);
    assert!(!gen.is_openai_compatible);

    let gen = LocalCommentaryGenerator::with_endpoint(
        "llama3".to_string(),
        5000,
        "http://localhost:8000/v1/chat/completions",
    );
    assert!(gen.is_openai_compatible);
}

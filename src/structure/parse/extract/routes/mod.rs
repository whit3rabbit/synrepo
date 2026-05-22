mod common;
mod csharp;
mod go;
mod js_ts;
mod jvm;
mod php;
mod python;
mod ruby;
mod rust;

use crate::structure::parse::{ExtractedEdge, ExtractedSymbol, Language};

use common::RouteCollector;

pub(super) fn extract_route_bindings(
    language: Language,
    _tree: &tree_sitter::Tree,
    content: &[u8],
) -> (Vec<ExtractedSymbol>, Vec<ExtractedEdge>) {
    let mut collector = RouteCollector::new(content);
    match language {
        Language::Rust => rust::collect(&mut collector),
        Language::Python => python::collect(&mut collector),
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            js_ts::collect(&mut collector)
        }
        Language::Java | Language::Kotlin => jvm::collect(&mut collector),
        Language::Go => go::collect(&mut collector),
        Language::CSharp => csharp::collect(&mut collector),
        Language::Php => php::collect(&mut collector),
        Language::Ruby => ruby::collect(&mut collector),
        _ => {}
    }
    collector.finish()
}

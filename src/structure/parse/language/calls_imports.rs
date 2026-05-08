// --- Call-site queries (stage 4: cross-file edge resolution) ---

// Rust call patterns:
//   0: (call_expression function: (identifier) @callee) -> Free function call
//   1: (call_expression function: (field_expression value: (_) @callee_prefix field: (field_identifier) @callee)) -> Method call
//   2: (call_expression function: (scoped_identifier path: (_) @callee_prefix name: (identifier) @callee)) -> Qualified call
const RUST_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (field_expression value: (_) @callee_prefix field: (field_identifier) @callee))
(call_expression function: (scoped_identifier path: (_) @callee_prefix name: (identifier) @callee))
"#;

// Pattern index -> CallMode (see RUST_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
//   2: Qualified call (has prefix)
const RUST_CALL_MODE_MAP: &[super::CallMode] = &[
    super::CallMode::Free,
    super::CallMode::Method,
    super::CallMode::Method,
];

// Python call patterns:
//   0: (call function: (identifier) @callee) -> Free function call
//   1: (call function: (attribute object: (_) @callee_prefix attribute: (identifier) @callee)) -> Method call
const PYTHON_CALL_QUERY: &str = r#"
(call function: (identifier) @callee)
(call function: (attribute object: (_) @callee_prefix attribute: (identifier) @callee))
"#;

// Pattern index -> CallMode (see PYTHON_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
const PYTHON_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];

// TypeScript call patterns:
//   0: (call_expression function: (identifier) @callee) -> Free function call
//   1: (call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee)) -> Method call
const TS_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee))
"#;

// Pattern index -> CallMode (see TS_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
const TS_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];

// --- Import/use queries (stage 4: cross-file edge resolution) ---

// Captures the full argument text of a `use_declaration`. The
// `scoped_identifier` node's text is the whole `::`-separated path
// (e.g., `std::collections::HashMap`, `crate::util::helper`), which
// stage 4's Rust resolver needs to map onto candidate module files.
// The bare-identifier arm still covers single-segment `use foo;`.
//
// Braced `use foo::{a, b};` fans out: one match per leaf, each emitting
// `@use_path` (the scoped prefix) plus `@use_item` (the leaf). The
// extractor joins them with `::` so the resolver sees the same shape as
// a bare `scoped_identifier` capture.
const RUST_IMPORT_QUERY: &str = r#"
(use_declaration argument: (identifier) @import_ref)
(use_declaration argument: (scoped_identifier) @import_ref)
(use_declaration argument: (scoped_use_list path: (_) @use_path list: (use_list (identifier) @use_item)))
"#;

// Python `from foo import bar` also captures `@import_name` so the
// extractor emits `foo.bar` alongside the bare `foo` module. The
// resolver tolerates unresolved paths; the dotted leaf exists so that
// a future stage-5 pass can resolve to a specific symbol.
const PYTHON_IMPORT_QUERY: &str = r#"
(import_statement name: (dotted_name) @import_ref)
(import_from_statement module_name: (dotted_name) @import_ref)
(import_from_statement module_name: (dotted_name) @import_ref name: (dotted_name) @import_name)
"#;

// `export { foo } from './bar'` is a re-export; the `source` shape is
// identical to an `import_statement`, so the resolver needs no change.
const TS_IMPORT_QUERY: &str = r#"
(import_statement source: (string (string_fragment) @import_ref))
(export_statement source: (string (string_fragment) @import_ref))
"#;

// Go call patterns:
//   0: (call_expression function: (identifier) @callee) -> Free function call
//   1: (call_expression function: (selector_expression operand: (_) @callee_prefix field: (field_identifier) @callee)) -> Method call
const GO_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (selector_expression operand: (_) @callee_prefix field: (field_identifier) @callee))
"#;

// Pattern index -> CallMode (see GO_CALL_QUERY patterns above):
//   0: Free function call
//   1: Method call (has prefix)
const GO_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];

const GO_IMPORT_QUERY: &str = r#"
(import_spec path: (interpreted_string_literal) @import_ref)
"#;

const JS_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (member_expression object: (_) @callee_prefix property: (property_identifier) @callee))
"#;
const JS_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const JS_IMPORT_QUERY: &str = r#"
(import_statement source: (string (string_fragment) @import_ref))
(export_statement source: (string (string_fragment) @import_ref))
"#;

const JAVA_CALL_QUERY: &str = r#"
(method_invocation name: (identifier) @callee)
(method_invocation object: (_) @callee_prefix name: (identifier) @callee)
"#;
const JAVA_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const JAVA_IMPORT_QUERY: &str = r#"
(import_declaration (scoped_identifier) @import_ref)
"#;

const KOTLIN_CALL_QUERY: &str = r#"
(call_expression (identifier) @callee)
(call_expression (navigation_expression (identifier) @callee))
"#;
const KOTLIN_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const KOTLIN_IMPORT_QUERY: &str = r#"
(import (identifier) @import_ref)
(import (qualified_identifier) @import_ref)
"#;

const CSHARP_CALL_QUERY: &str = r#"
(invocation_expression function: (identifier) @callee)
(invocation_expression function: (member_access_expression name: (identifier) @callee))
"#;
const CSHARP_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const CSHARP_IMPORT_QUERY: &str = r#"
(using_directive (identifier) @import_ref)
(using_directive (qualified_name) @import_ref)
"#;

const PHP_CALL_QUERY: &str = r#"
(function_call_expression function: (name) @callee)
(member_call_expression name: (name) @callee)
"#;
const PHP_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const PHP_IMPORT_QUERY: &str = r#"
(namespace_use_clause (name) @import_ref)
"#;

const RUBY_CALL_QUERY: &str = r#"
(call method: (identifier) @callee)
"#;
const RUBY_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free];
const RUBY_IMPORT_QUERY: &str = r#"
(call method: (identifier) @method_name (#eq? @method_name "require") arguments: (argument_list (string (string_content) @import_ref)))
"#;

const SWIFT_CALL_QUERY: &str = r#"
(call_expression (identifier) @callee)
(call_expression (navigation_expression suffix: (simple_identifier) @callee))
"#;
const SWIFT_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const SWIFT_IMPORT_QUERY: &str = r#"
(import_declaration (identifier) @import_ref)
"#;

const C_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
"#;
const C_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free];
const C_IMPORT_QUERY: &str = r#"
(preproc_include path: (system_lib_string) @import_ref)
(preproc_include path: (string_literal) @import_ref)
"#;

const CPP_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (field_expression field: (field_identifier) @callee))
"#;
const CPP_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Free, super::CallMode::Method];
const CPP_IMPORT_QUERY: &str = r#"
(preproc_include path: (system_lib_string) @import_ref)
(preproc_include path: (string_literal) @import_ref)
"#;

const DART_CALL_QUERY: &str = r#"
(unconditional_assignable_selector (identifier) @callee)
(assignable_expression (identifier) @callee)
"#;
const DART_CALL_MODE_MAP: &[super::CallMode] = &[super::CallMode::Method, super::CallMode::Free];
const DART_IMPORT_QUERY: &str = r#"
(uri) @import_ref
"#;

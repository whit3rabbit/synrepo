//! Malformed-source behavior pins for `parse_file`.
//!
//! The documented contract lives in `parse/mod.rs` ("Parser invariants").
//! These tests assert:
//!   * unsupported extension → `Ok(None)`
//!   * supported extension + malformed source → `Ok(Some(ParseOutput))`, no panic, deterministic
//!   * empty input → `Ok(Some(ParseOutput))` with no symbols, no panic
//!
//! Runtime must stay permissive so everyday user repos with half-rewritten
//! or generated files still compile through. Strictness is scoped to tests.

use super::{parse_file, Language};
use std::path::Path;

// ── Task 6.1: unsupported extension → None ───────────────────────────────────

#[test]
fn unsupported_extension_returns_none() {
    for path in [
        "config.yaml",
        "README.md",
        "binary.bin",
        "noext",
        "data.csv",
        "lockfile.toml",
    ] {
        let result = parse_file(Path::new(path), b"anything here").unwrap();
        assert!(
            result.is_none(),
            "parse_file({path}) must return None for unsupported extension"
        );
    }
}

// ── Task 6.2: malformed source returns Some(ParseOutput), no panic ───────────

#[test]
fn malformed_rust_source_returns_best_effort_output() {
    // Deliberately broken: unclosed brace, missing types. Parser must not
    // panic and must return a ParseOutput.
    let source = b"pub fn broken(a: i32, b: -> String { format!(\n";
    let output = parse_file(Path::new("src/broken.rs"), source)
        .unwrap()
        .expect("supported extension must return Some even on malformed source");
    assert_eq!(output.language, Language::Rust);
    // Bounded output: symbol count must not be negative or wildly large.
    // We don't pin the exact number because grammar upgrades may recover
    // more from broken input. Upper bound just sanity-checks.
    assert!(output.symbols.len() < 100, "suspiciously many symbols");
}

#[test]
fn malformed_python_source_returns_best_effort_output() {
    let source = b"def broken(:\n    return\n";
    let output = parse_file(Path::new("broken.py"), source).unwrap().unwrap();
    assert_eq!(output.language, Language::Python);
}

#[test]
fn malformed_typescript_source_returns_best_effort_output() {
    let source = b"function broken(a: b c: ): { return\n";
    let output = parse_file(Path::new("src/broken.ts"), source)
        .unwrap()
        .unwrap();
    assert_eq!(output.language, Language::TypeScript);
}

#[test]
fn malformed_tsx_source_returns_best_effort_output() {
    let source = b"export function App() { return <div; } \n";
    let output = parse_file(Path::new("src/broken.tsx"), source)
        .unwrap()
        .unwrap();
    assert_eq!(output.language, Language::Tsx);
}

#[test]
fn malformed_go_source_returns_best_effort_output() {
    let source = b"package main\nfunc broken( {\n";
    let output = parse_file(Path::new("broken.go"), source).unwrap().unwrap();
    assert_eq!(output.language, Language::Go);
}

// ── Task 6.3: determinism for identical bytes ────────────────────────────────

fn assert_deterministic(path: &str, source: &[u8]) {
    let a = parse_file(Path::new(path), source).unwrap().unwrap();
    let b = parse_file(Path::new(path), source).unwrap().unwrap();

    assert_eq!(a.language, b.language);
    assert_eq!(a.symbols.len(), b.symbols.len(), "symbol count drift");
    for (s1, s2) in a.symbols.iter().zip(b.symbols.iter()) {
        assert_eq!(s1.qualified_name, s2.qualified_name);
        assert_eq!(s1.display_name, s2.display_name);
        assert_eq!(s1.kind, s2.kind);
        assert_eq!(s1.body_byte_range, s2.body_byte_range);
        assert_eq!(s1.body_hash, s2.body_hash);
        assert_eq!(s1.signature, s2.signature);
        assert_eq!(s1.doc_comment, s2.doc_comment);
    }
    assert_eq!(
        a.call_refs.len(),
        b.call_refs.len(),
        "call_refs count drift"
    );
    for (r1, r2) in a.call_refs.iter().zip(b.call_refs.iter()) {
        assert_eq!(r1.callee_name, r2.callee_name);
    }
    assert_eq!(
        a.import_refs.len(),
        b.import_refs.len(),
        "import_refs count drift"
    );
    for (r1, r2) in a.import_refs.iter().zip(b.import_refs.iter()) {
        assert_eq!(r1.module_ref, r2.module_ref);
    }
}

#[test]
fn parse_file_is_deterministic_across_runs() {
    assert_deterministic(
        "src/a.rs",
        b"pub fn f(x: i32) -> i32 { x + 1 }\npub struct S;\n",
    );
    assert_deterministic("b.py", b"def f(x):\n    return x\nclass C: pass\n");
    assert_deterministic(
        "src/c.ts",
        b"export function f(x: number): number { return x; }\nexport class C {}\n",
    );
    assert_deterministic(
        "src/d.tsx",
        b"import * as React from 'react';\nexport function A() { return <div />; }\n",
    );
    assert_deterministic("e.go", b"package main\nfunc F() string { return \"x\" }\n");
}

// ── Task 6.4: empty input ────────────────────────────────────────────────────

#[test]
fn empty_input_returns_some_with_no_symbols_per_language() {
    for path in ["empty.rs", "empty.py", "empty.ts", "empty.tsx", "empty.go"] {
        let out = parse_file(Path::new(path), b"").unwrap();
        let out = out.unwrap_or_else(|| panic!("{path}: expected Some on empty input"));
        assert!(
            out.symbols.is_empty(),
            "{path}: expected empty symbols, got {:?}",
            out.symbols
                .iter()
                .map(|s| &s.display_name)
                .collect::<Vec<_>>()
        );
        assert!(out.call_refs.is_empty(), "{path}: call_refs must be empty");
        assert!(
            out.import_refs.is_empty(),
            "{path}: import_refs must be empty"
        );
    }
}

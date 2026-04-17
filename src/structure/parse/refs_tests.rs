//! First-class tests for `ParseOutput.call_refs` and `ParseOutput.import_refs`.
//!
//! Stage 4 silently skips unresolved call / import references, which hides
//! parser regressions behind "oh, unresolved, fine". These tests assert the
//! parser-side outputs directly so a broken query fails here, not later.

use super::{parse_file, Language};
use std::path::Path;

fn parse(path: &str, source: &[u8]) -> super::ParseOutput {
    parse_file(Path::new(path), source)
        .unwrap_or_else(|e| panic!("parse_file errored on {path}: {e}"))
        .unwrap_or_else(|| panic!("parse_file returned None on {path}"))
}

fn callee_names(output: &super::ParseOutput) -> Vec<&str> {
    output
        .call_refs
        .iter()
        .map(|r| r.callee_name.as_str())
        .collect()
}

fn import_paths(output: &super::ParseOutput) -> Vec<&str> {
    output
        .import_refs
        .iter()
        .map(|r| r.module_ref.as_str())
        .collect()
}

// ── Task 5.1: call_refs per language ─────────────────────────────────────────

#[test]
fn rust_call_refs_capture_local_callee_names() {
    let source = br#"
fn outer() {
    helper();                   // bare call
    self.method();              // field_expression call
    crate::util::named();       // scoped_identifier call
}
fn helper() {}
"#;
    let output = parse(&"src/rust_calls.rs", source);
    let calls = callee_names(&output);
    for wanted in ["helper", "method", "named"] {
        assert!(
            calls.contains(&wanted),
            "Rust call_refs missing `{wanted}`; got: {calls:?}"
        );
    }
}

#[test]
fn python_call_refs_capture_local_callee_names() {
    let source = b"
def main():
    greet('x')
    self.method()
";
    let output = parse(&"calls.py", source);
    let calls = callee_names(&output);
    assert!(calls.contains(&"greet"), "python calls: {calls:?}");
    assert!(calls.contains(&"method"), "python calls: {calls:?}");
}

#[test]
fn typescript_call_refs_capture_local_callee_names() {
    let source = b"
function main(): void {
    greet('x');
    obj.method();
}
";
    let output = parse(&"src/calls.ts", source);
    let calls = callee_names(&output);
    assert!(calls.contains(&"greet"), "ts calls: {calls:?}");
    assert!(calls.contains(&"method"), "ts calls: {calls:?}");
}

#[test]
fn tsx_call_refs_capture_local_callee_names() {
    let source = b"
import * as React from 'react';
export function App() {
    setup();
    ctx.attach();
    return <div />;
}
";
    let output = parse(&"src/app.tsx", source);
    assert_eq!(output.language, Language::Tsx);
    let calls = callee_names(&output);
    assert!(calls.contains(&"setup"), "tsx calls: {calls:?}");
    assert!(calls.contains(&"attach"), "tsx calls: {calls:?}");
}

#[test]
fn go_call_refs_capture_local_callee_names() {
    let source = b"
package main

func main() {
    fmt.Println(\"hi\")
    Greet(\"there\")
}

func Greet(name string) string { return name }
";
    let output = parse(&"src/calls.go", source);
    let calls = callee_names(&output);
    assert!(calls.contains(&"Println"), "go calls: {calls:?}");
    assert!(calls.contains(&"Greet"), "go calls: {calls:?}");
}

// ── Task 5.2: import_refs per language ───────────────────────────────────────

#[test]
fn rust_import_refs_capture_last_name_of_use_paths() {
    let source = b"
use std::collections::HashMap;
use serde::Serialize;
use crate::util::helper;
";
    let output = parse(&"src/rust_imports.rs", source);
    let refs = import_paths(&output);
    for wanted in ["HashMap", "Serialize", "helper"] {
        assert!(
            refs.contains(&wanted),
            "Rust import_refs missing `{wanted}`; got: {refs:?}"
        );
    }
}

#[test]
fn python_import_refs_capture_dotted_module_names() {
    let source = b"
import os
import collections.abc
from typing import List
";
    let output = parse(&"imports.py", source);
    let refs = import_paths(&output);
    assert!(refs.iter().any(|r| *r == "os"), "py imports: {refs:?}");
    assert!(
        refs.iter().any(|r| *r == "collections.abc"),
        "py dotted import: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| *r == "typing"),
        "py from-import: {refs:?}"
    );
}

#[test]
fn typescript_import_refs_capture_raw_module_path() {
    let source = b"
import { join } from 'path';
import def from './local';
";
    let output = parse(&"src/imports.ts", source);
    let refs = import_paths(&output);
    assert!(refs.contains(&"path"), "ts imports: {refs:?}");
    assert!(refs.contains(&"./local"), "ts relative imports: {refs:?}");
}

#[test]
fn tsx_import_refs_capture_raw_module_path() {
    let source = b"
import * as React from 'react';
import { Helper } from './helper';
";
    let output = parse(&"src/imports.tsx", source);
    assert_eq!(output.language, Language::Tsx);
    let refs = import_paths(&output);
    assert!(refs.contains(&"react"), "tsx imports: {refs:?}");
    assert!(refs.contains(&"./helper"), "tsx relative: {refs:?}");
}

#[test]
fn go_import_refs_capture_quoted_module_paths() {
    let source = b"
package main

import (
    \"fmt\"
    \"os\"
)
";
    let output = parse(&"src/imports.go", source);
    let refs = import_paths(&output);
    // Go's `interpreted_string_literal` capture includes the surrounding
    // double quotes; stage 4 is responsible for stripping them.
    assert!(
        refs.iter().any(|r| r.contains("fmt")),
        "go imports: {refs:?}"
    );
    assert!(
        refs.iter().any(|r| r.contains("os")),
        "go imports: {refs:?}"
    );
}

// ── Task 5.3: intentionally unsupported import forms are absent ──────────────

#[test]
fn rust_braced_use_group_is_not_captured_phase1() {
    // `use foo::{bar, baz};` uses a `use_list` node, which the current
    // Rust import query intentionally does not match. Stage 4 only
    // resolves the final identifier of scoped or bare `use` paths, so
    // capturing braced lists here would generate noise. Revisit when
    // stage-4 resolution improves.
    let source = b"use std::collections::{HashMap, HashSet};\n";
    let output = parse(&"src/rust_braced.rs", source);
    let refs = import_paths(&output);
    assert!(
        !refs.iter().any(|r| *r == "HashMap" || *r == "HashSet"),
        "Rust braced-use names must NOT appear in import_refs (phase-1). got: {refs:?}"
    );
}

#[test]
fn python_import_from_names_are_not_captured_phase1() {
    // `from foo import bar` captures only the module (`foo`). The imported
    // symbol (`bar`) is intentionally not captured because stage 4 resolves
    // imports to target files, not to specific symbols in those files.
    let source = b"from typing import List, Dict\n";
    let output = parse(&"py_fromimport.py", source);
    let refs = import_paths(&output);
    assert!(
        refs.contains(&"typing"),
        "python from-import base: {refs:?}"
    );
    assert!(
        !refs.iter().any(|r| *r == "List" || *r == "Dict"),
        "python imported-name symbols must NOT appear as import_refs (phase-1). got: {refs:?}"
    );
}

#[test]
fn typescript_reexport_forms_are_not_captured_phase1() {
    // `export { foo } from './bar'` is an `export_statement`, not an
    // `import_statement`. The current TS import query matches only
    // import_statement, so re-exports are intentionally skipped.
    let source = b"export { helper } from './helper';\n";
    let output = parse(&"src/reexport.ts", source);
    let refs = import_paths(&output);
    assert!(
        !refs.contains(&"./helper"),
        "TS re-export source must NOT appear in import_refs (phase-1); got: {refs:?}"
    );
}

#[test]
fn tsx_reexport_forms_are_not_captured_phase1() {
    let source = b"export { Card } from './card';\n";
    let output = parse(&"src/reexport.tsx", source);
    assert_eq!(output.language, Language::Tsx);
    let refs = import_paths(&output);
    assert!(
        !refs.contains(&"./card"),
        "TSX re-export source must NOT appear in import_refs (phase-1); got: {refs:?}"
    );
}

#[test]
fn go_dot_import_alias_is_skipped_phase1() {
    // Go `import . "fmt"` still captures the path via interpreted_string_literal;
    // the current query keeps the behavior, but the *alias* (`.`) is
    // intentionally not captured as its own entry. This test pins that
    // there is one entry per import line, not two (path + alias).
    let source = b"
package main

import . \"fmt\"

func main() { Println(\"hi\") }
";
    let output = parse(&"src/dot_import.go", source);
    let refs = import_paths(&output);
    assert_eq!(
        refs.iter().filter(|r| r.contains("fmt")).count(),
        1,
        "Go dot-import must produce exactly one import_ref for the path; got: {refs:?}"
    );
    assert!(
        !refs.iter().any(|r| *r == "."),
        "Go dot-import alias must NOT appear as its own import_ref; got: {refs:?}"
    );
}

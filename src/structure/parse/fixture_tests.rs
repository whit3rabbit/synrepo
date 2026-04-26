//! Per-language parser fixture coverage.
//!
//! Each supported `Language` has at least one in-tree fixture with a known
//! set of expected symbols. The registry below is the coverage surface:
//! adding a new `Language` variant triggers the `fixtures_cover_every_supported_language`
//! test to fail until a fixture is registered.
//!
//! Fixtures are embedded Rust strings rather than `.scm` or on-disk files —
//! see design.md Decision 6 for the rationale.

use super::{parse_file, ExtractedSymbol, Language};
use crate::structure::graph::SymbolKind;
use std::path::Path;

/// Test fixture for a single supported language. One language may have
/// multiple fixtures when a construct does not fit naturally into a single
/// source (for example, TSX components live alongside `.ts` files).
struct Fixture {
    /// Language this fixture exercises.
    language: Language,
    /// Label used in assertion diagnostics.
    name: &'static str,
    /// Virtual path passed to `parse_file` — drives extension dispatch.
    path: &'static str,
    /// Fixture source bytes.
    source: &'static [u8],
    /// Symbols that MUST be extracted from the fixture. The test asserts
    /// each `(display_name, kind)` pair is present. Additional symbols are
    /// permitted (grammar upgrades may introduce new patterns).
    expected_symbols: &'static [(&'static str, SymbolKind)],
    /// Import paths that MUST appear verbatim in `ParseOutput.import_refs`.
    expected_imports: &'static [&'static str],
}

// ── Rust fixture (task 3.1) ──────────────────────────────────────────────────

const RUST_SOURCE: &[u8] = br#"
//! Rust fixture covering the `Language::Rust` definition-query patterns.

use std::fmt::Debug;

/// A free function.
pub fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

/// A struct.
pub struct Greeter {
    name: String,
}

/// A trait.
pub trait Greetable {
    fn greet(&self) -> String;
}

/// An enum.
pub enum Status {
    Active,
    Inactive,
}

/// A type alias.
pub type Name = String;

/// A submodule.
pub mod helpers {
    pub fn noop() {}
}

/// A constant.
pub const MAX: usize = 100;

/// A static.
pub static FLAG: bool = false;

impl Greeter {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}
"#;

// ── Python fixture (task 3.2) ────────────────────────────────────────────────

const PYTHON_SOURCE: &[u8] = br#"
"""Python fixture covering functions, classes, methods, decorators, imports."""

import os
from typing import List

def greet(name):
    """Greet someone by name."""
    return f"Hello, {name}!"

class Greeter:
    """A greeter."""

    def __init__(self, prefix: str):
        self.prefix = prefix

    @staticmethod
    def default():
        return Greeter("Hi")

    def greet(self, name: str) -> str:
        return f"{self.prefix}, {name}"
"#;

// ── TypeScript fixture (task 3.3) ────────────────────────────────────────────

const TYPESCRIPT_SOURCE: &[u8] = br#"
import { join } from 'path';
import * as fs from 'fs';

export function greet(name: string): string {
    return `Hello, ${name}`;
}

export interface Named {
    name: string;
}

export type Greeting = string;

export class Greeter implements Named {
    name: string;
    constructor(name: string) { this.name = name; }
    greet(): Greeting { return `Hi, ${this.name}`; }
}

export const Shape = class {
    area(): number { return 0; }
};
"#;

// ── TSX fixture (task 3.4) ───────────────────────────────────────────────────

const TSX_SOURCE: &[u8] = br#"
import * as React from 'react';

export interface GreetingProps {
    name: string;
}

export function Greeting(props: GreetingProps) {
    return <div className="greeting">Hello, {props.name}</div>;
}

export class GreetingCard extends React.Component<GreetingProps> {
    render() {
        return <Greeting name={this.props.name} />;
    }
}
"#;

// ── Go fixture (task 3.5) ────────────────────────────────────────────────────

const GO_SOURCE: &[u8] = br#"
package main

import (
    "fmt"
    "os"
)

// Greeter holds greeter state.
type Greeter struct {
    Name string
}

// Namer is an interface.
type Namer interface {
    GetName() string
}

// MaxRetries documents a constant.
const MaxRetries = 3

// Greet greets the caller.
func Greet(name string) string {
    return fmt.Sprintf("Hello, %s!", name)
}

// GetName returns the greeter's name.
func (g *Greeter) GetName() string {
    return g.Name
}

func main() {
    _ = os.Args
}
"#;

const JS_SOURCE: &[u8] = br#"
import * as fs from 'fs';
export function greet() {}
class Greeter {}
"#;

const JAVA_SOURCE: &[u8] = br#"
import java.util.List;
class Greeter {
    void greet() {}
}
"#;

const KOTLIN_SOURCE: &[u8] = br#"
import java.util.List
class Greeter {
    fun greet() {}
}
"#;

const CSHARP_SOURCE: &[u8] = br#"
using System;
class Greeter {
    void Greet() {}
}
"#;

const PHP_SOURCE: &[u8] = br#"<?php
use Exception;
class Greeter {
    function greet() {}
}
"#;

const RUBY_SOURCE: &[u8] = br#"
require 'json'
class Greeter
  def greet
  end
end
"#;

const SWIFT_SOURCE: &[u8] = br#"
import Foundation
class Greeter {
    func greet() {}
}
"#;

const C_SOURCE: &[u8] = br#"
#include <stdio.h>
void greet() {}
"#;

const CPP_SOURCE: &[u8] = br#"
#include <iostream>
class Greeter {
    void greet() {}
};
"#;

const DART_SOURCE: &[u8] = br#"
import 'dart:core';
class Greeter {
    void greet() {}
}
"#;

const FIXTURES: &[Fixture] = &[
    Fixture {
        language: Language::Rust,
        name: "rust_definitions",
        path: "src/fixture.rs",
        source: RUST_SOURCE,
        expected_symbols: &[
            ("greet", SymbolKind::Function),
            ("Greeter", SymbolKind::Class),
            ("Greetable", SymbolKind::Trait),
            ("Status", SymbolKind::Class),
            ("Name", SymbolKind::Type),
            ("helpers", SymbolKind::Module),
            ("MAX", SymbolKind::Constant),
            ("FLAG", SymbolKind::Constant),
            ("new", SymbolKind::Method),
        ],
        expected_imports: &["Debug"],
    },
    Fixture {
        language: Language::Python,
        name: "python_definitions",
        path: "src/fixture.py",
        source: PYTHON_SOURCE,
        expected_symbols: &[
            ("greet", SymbolKind::Function),
            ("Greeter", SymbolKind::Class),
            ("__init__", SymbolKind::Method),
            ("default", SymbolKind::Method),
        ],
        expected_imports: &["os", "typing"],
    },
    Fixture {
        language: Language::TypeScript,
        name: "typescript_definitions",
        path: "src/fixture.ts",
        source: TYPESCRIPT_SOURCE,
        expected_symbols: &[
            ("greet", SymbolKind::Function),
            ("Named", SymbolKind::Trait),
            ("Greeting", SymbolKind::Type),
            ("Greeter", SymbolKind::Class),
            ("greet", SymbolKind::Method),
            ("Shape", SymbolKind::Class),
            ("area", SymbolKind::Method),
        ],
        expected_imports: &["path", "fs"],
    },
    Fixture {
        language: Language::Tsx,
        name: "tsx_definitions",
        path: "src/fixture.tsx",
        source: TSX_SOURCE,
        expected_symbols: &[
            ("GreetingProps", SymbolKind::Trait),
            ("Greeting", SymbolKind::Function),
            ("GreetingCard", SymbolKind::Class),
            ("render", SymbolKind::Method),
        ],
        expected_imports: &["react"],
    },
    Fixture {
        language: Language::Go,
        name: "go_definitions",
        path: "src/fixture.go",
        source: GO_SOURCE,
        expected_symbols: &[
            ("Greeter", SymbolKind::Class),
            ("Namer", SymbolKind::Interface),
            ("MaxRetries", SymbolKind::Constant),
            ("Greet", SymbolKind::Function),
            ("GetName", SymbolKind::Method),
            ("main", SymbolKind::Function),
        ],
        expected_imports: &["fmt", "os"],
    },
    Fixture {
        language: Language::JavaScript,
        name: "javascript_definitions",
        path: "src/fixture.js",
        source: JS_SOURCE,
        expected_symbols: &[
            ("greet", SymbolKind::Function),
            ("Greeter", SymbolKind::Class),
        ],
        expected_imports: &["fs"],
    },
    Fixture {
        language: Language::Java,
        name: "java_definitions",
        path: "src/fixture.java",
        source: JAVA_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["java.util.List"],
    },
    Fixture {
        language: Language::Kotlin,
        name: "kotlin_definitions",
        path: "src/fixture.kt",
        source: KOTLIN_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["java.util.List"],
    },
    Fixture {
        language: Language::CSharp,
        name: "csharp_definitions",
        path: "src/fixture.cs",
        source: CSHARP_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["System"],
    },
    Fixture {
        language: Language::Php,
        name: "php_definitions",
        path: "src/fixture.php",
        source: PHP_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["Exception"],
    },
    Fixture {
        language: Language::Ruby,
        name: "ruby_definitions",
        path: "src/fixture.rb",
        source: RUBY_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["json"],
    },
    Fixture {
        language: Language::Swift,
        name: "swift_definitions",
        path: "src/fixture.swift",
        source: SWIFT_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["Foundation"],
    },
    Fixture {
        language: Language::C,
        name: "c_definitions",
        path: "src/fixture.c",
        source: C_SOURCE,
        expected_symbols: &[("greet", SymbolKind::Function)],
        expected_imports: &["stdio.h"],
    },
    Fixture {
        language: Language::Cpp,
        name: "cpp_definitions",
        path: "src/fixture.cpp",
        source: CPP_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["iostream"],
    },
    Fixture {
        language: Language::Dart,
        name: "dart_definitions",
        path: "src/fixture.dart",
        source: DART_SOURCE,
        expected_symbols: &[("Greeter", SymbolKind::Class)],
        expected_imports: &["dart:core"],
    },
];

fn find_symbol<'a>(
    symbols: &'a [ExtractedSymbol],
    name: &str,
    kind: SymbolKind,
) -> Option<&'a ExtractedSymbol> {
    symbols
        .iter()
        .find(|s| s.display_name == name && s.kind == kind)
}

fn assert_fixture(fixture: &Fixture) {
    let output = parse_file(Path::new(fixture.path), fixture.source)
        .unwrap_or_else(|e| panic!("{}: parse_file errored: {e}", fixture.name))
        .unwrap_or_else(|| panic!("{}: parse_file returned None", fixture.name));

    assert_eq!(
        output.language, fixture.language,
        "{}: expected language {:?}, got {:?}",
        fixture.name, fixture.language, output.language,
    );

    for (name, kind) in fixture.expected_symbols {
        assert!(
            find_symbol(&output.symbols, name, *kind).is_some(),
            "{}: missing expected symbol {name}:{kind:?}. Extracted symbols: {:?}",
            fixture.name,
            output
                .symbols
                .iter()
                .map(|s| (s.display_name.as_str(), s.kind))
                .collect::<Vec<_>>(),
        );
    }

    for wanted in fixture.expected_imports {
        assert!(
            output
                .import_refs
                .iter()
                .any(|r| r.module_ref.contains(wanted)),
            "{}: expected import_refs to contain '{wanted}'. Got: {:?}",
            fixture.name,
            output
                .import_refs
                .iter()
                .map(|r| r.module_ref.as_str())
                .collect::<Vec<_>>(),
        );
    }
}

// ── Task 3.1–3.5: per-language fixture assertions ────────────────────────────

#[test]
fn rust_fixture_parses() {
    assert_fixture(&FIXTURES[0]);
}

#[test]
fn python_fixture_parses() {
    assert_fixture(&FIXTURES[1]);
}

#[test]
fn typescript_fixture_parses() {
    assert_fixture(&FIXTURES[2]);
}

#[test]
fn tsx_fixture_parses_with_language_tsx() {
    // Task 3.4 explicitly requires asserting `ParseOutput.language == Language::Tsx`
    // so TSX does not silently fall back to TypeScript parsing.
    let fixture = &FIXTURES[3];
    let output = parse_file(Path::new(fixture.path), fixture.source)
        .unwrap()
        .unwrap();
    assert_eq!(output.language, Language::Tsx);
    assert_fixture(fixture);
}

#[test]
fn go_fixture_parses() {
    assert_fixture(&FIXTURES[4]);
}

// ── Task 3.6: fixture registry covers every supported language ───────────────

#[test]
fn fixtures_cover_every_supported_language() {
    for &lang in Language::supported() {
        assert!(
            FIXTURES.iter().any(|f| f.language == lang),
            "Language::{:?} has no fixture registered in FIXTURES. \
             Add a fixture in src/structure/parse/fixture_tests.rs so the \
             new variant gets explicit parser extraction coverage.",
            lang,
        );
    }
}

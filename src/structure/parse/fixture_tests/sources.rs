// Fixture source strings live separately so parser test assertions stay small.

// ── Rust fixture (task 3.1) ──────────────────────────────────────────────────

pub(super) const RUST_SOURCE: &[u8] = br#"
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

pub(super) const PYTHON_SOURCE: &[u8] = br#"
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

pub(super) const TYPESCRIPT_SOURCE: &[u8] = br#"
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

pub(super) const TSX_SOURCE: &[u8] = br#"
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

pub(super) const GO_SOURCE: &[u8] = br#"
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

pub(super) const JS_SOURCE: &[u8] = br#"
import * as fs from 'fs';
export function greet() {}
class Greeter {}
"#;

pub(super) const JAVA_SOURCE: &[u8] = br#"
import java.util.List;
class Greeter {
    void greet() {}
}
"#;

pub(super) const KOTLIN_SOURCE: &[u8] = br#"
import java.util.List
class Greeter {
    fun greet() {}
}
"#;

pub(super) const CSHARP_SOURCE: &[u8] = br#"
using System;
class Greeter {
    void Greet() {}
}
"#;

pub(super) const PHP_SOURCE: &[u8] = br#"<?php
use Exception;
class Greeter {
    function greet() {}
}
"#;

pub(super) const RUBY_SOURCE: &[u8] = br#"
require 'json'
class Greeter
  def greet
  end
end
"#;

pub(super) const SWIFT_SOURCE: &[u8] = br#"
import Foundation
class Greeter {
    func greet() {}
}
"#;

pub(super) const C_SOURCE: &[u8] = br#"
#include <stdio.h>
void greet() {}
"#;

pub(super) const CPP_SOURCE: &[u8] = br#"
#include <iostream>
class Greeter {
    void greet() {}
};
"#;

pub(super) const DART_SOURCE: &[u8] = br#"
import 'dart:core';
class Greeter {
    void greet() {}
}
"#;

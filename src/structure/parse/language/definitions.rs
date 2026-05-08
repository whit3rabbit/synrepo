// Pattern index -> kind (see RUST_KIND_MAP):
//   0: function_item -> Function
//   1: struct_item   -> Class
//   2: enum_item     -> Class
//   3: trait_item    -> Trait
//   4: type_item     -> Type
//   5: mod_item      -> Module
//   6: const_item    -> Constant
//   7: static_item   -> Constant
const RUST_DEFINITION_QUERY: &str = r#"
(function_item name: (identifier) @name) @item
(struct_item name: (type_identifier) @name) @item
(enum_item name: (type_identifier) @name) @item
(trait_item name: (type_identifier) @name) @item
(type_item name: (type_identifier) @name) @item
(mod_item name: (identifier) @name) @item
(const_item name: (identifier) @name) @item
(static_item name: (identifier) @name) @item
"#;

const RUST_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Type,
    SymbolKind::Module,
    SymbolKind::Constant,
    SymbolKind::Constant,
];

// Pattern index -> kind (see PYTHON_KIND_MAP):
//   0: function_definition -> Function
//   1: class_definition    -> Class
const PYTHON_DEFINITION_QUERY: &str = r#"
(function_definition name: (identifier) @name) @item
(class_definition name: (identifier) @name) @item
"#;

const PYTHON_KIND_MAP: &[SymbolKind] = &[SymbolKind::Function, SymbolKind::Class];

// Pattern index -> kind (see TS_KIND_MAP):
//   0: function_declaration              -> Function
//   1: class_declaration                 -> Class
//   2: interface_declaration             -> Trait
//   3: type_alias_declaration            -> Type
//   4: method_definition                 -> Method
//   5: abstract_method_signature         -> Method
//   6: variable_declarator -> (class)     -> Class   (class-expression bound to a name)
const TS_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(interface_declaration name: (type_identifier) @name) @item
(type_alias_declaration name: (type_identifier) @name) @item
(method_definition name: (property_identifier) @name) @item
(abstract_method_signature name: (property_identifier) @name) @item
(variable_declarator name: (identifier) @name value: (class) @item)
"#;

const TS_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Type,
    SymbolKind::Method,
    SymbolKind::Method,
    SymbolKind::Class,
];

// --- Go queries ---

// Pattern index -> kind (see GO_KIND_MAP):
//   0: function_declaration -> Function
//   1: method_declaration   -> Method
//   2: interface type_spec  -> Interface
//   3: struct type_spec     -> Class
//   4: const_spec           -> Constant
//   5: var_spec             -> Constant
const GO_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(method_declaration name: (field_identifier) @name) @item
(type_spec name: (type_identifier) @name type: (interface_type)) @item
(type_spec name: (type_identifier) @name type: (struct_type)) @item
(const_spec name: (identifier) @name) @item
(var_spec name: (identifier) @name) @item
"#;

const GO_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Interface,
    SymbolKind::Class,
    SymbolKind::Constant,
    SymbolKind::Constant,
];

// --- JavaScript queries ---
const JS_DEFINITION_QUERY: &str = r#"
(function_declaration name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(method_definition name: (property_identifier) @name) @item
(variable_declarator name: (identifier) @name value: (arrow_function)) @item
"#;
const JS_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Method,
    SymbolKind::Function,
];

// --- Java queries ---
const JAVA_DEFINITION_QUERY: &str = r#"
(method_declaration name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(interface_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(annotation_type_declaration name: (identifier) @name) @item
"#;
const JAVA_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Class,
    SymbolKind::Type,
];

// --- Kotlin queries ---
const KOTLIN_DEFINITION_QUERY: &str = r#"
(class_declaration name: (identifier) @name) @item
(function_declaration name: (identifier) @name) @item
(object_declaration name: (identifier) @name) @item
"#;
const KOTLIN_KIND_MAP: &[SymbolKind] =
    &[SymbolKind::Class, SymbolKind::Function, SymbolKind::Class];

// --- C# queries ---
const CSHARP_DEFINITION_QUERY: &str = r#"
(method_declaration name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(interface_declaration name: (identifier) @name) @item
(struct_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(delegate_declaration name: (identifier) @name) @item
"#;
const CSHARP_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Type,
];

// --- PHP queries ---
const PHP_DEFINITION_QUERY: &str = r#"
(function_definition name: (name) @name) @item
(class_declaration name: (name) @name) @item
(interface_declaration name: (name) @name) @item
(trait_declaration name: (name) @name) @item
(method_declaration name: (name) @name) @item
"#;
const PHP_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Interface,
    SymbolKind::Trait,
    SymbolKind::Method,
];

// --- Ruby queries ---
const RUBY_DEFINITION_QUERY: &str = r#"
(method name: (identifier) @name) @item
(singleton_method name: (identifier) @name) @item
(class name: (constant) @name) @item
(module name: (constant) @name) @item
"#;
const RUBY_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Method,
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Module,
];

// --- Swift queries ---
const SWIFT_DEFINITION_QUERY: &str = r#"
(function_declaration name: (simple_identifier) @name) @item
(class_declaration name: (type_identifier) @name) @item
(protocol_declaration name: (type_identifier) @name) @item
"#;
const SWIFT_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Interface,
];

// --- C queries ---
const C_DEFINITION_QUERY: &str = r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @item
(struct_specifier name: (type_identifier) @name) @item
(enum_specifier name: (type_identifier) @name) @item
"#;
const C_KIND_MAP: &[SymbolKind] = &[SymbolKind::Function, SymbolKind::Class, SymbolKind::Class];

// --- C++ queries ---
const CPP_DEFINITION_QUERY: &str = r#"
(function_definition declarator: (function_declarator declarator: (identifier) @name)) @item
(function_definition declarator: (function_declarator declarator: (field_identifier) @name)) @item
(class_specifier name: (type_identifier) @name) @item
(struct_specifier name: (type_identifier) @name) @item
(enum_specifier name: (type_identifier) @name) @item
"#;
const CPP_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Class,
];

// --- Dart queries ---
const DART_DEFINITION_QUERY: &str = r#"
(function_signature name: (identifier) @name) @item
(class_declaration name: (identifier) @name) @item
(enum_declaration name: (identifier) @name) @item
(mixin_declaration name: (identifier) @name) @item
(extension_declaration name: (identifier) @name) @item
"#;
const DART_KIND_MAP: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Class,
    SymbolKind::Class,
    SymbolKind::Trait,
    SymbolKind::Class,
];

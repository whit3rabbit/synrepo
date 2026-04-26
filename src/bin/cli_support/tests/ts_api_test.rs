use std::any::Any;

fn main() {
    let _ = tree_sitter_javascript::LANGUAGE;
    let _ = tree_sitter_java::LANGUAGE;
    let _ = tree_sitter_kotlin_ng::LANGUAGE;
    let _ = tree_sitter_c_sharp::LANGUAGE;
    let _ = tree_sitter_php::LANGUAGE_PHP; // need to check
    let _ = tree_sitter_ruby::LANGUAGE;
    let _ = tree_sitter_swift::LANGUAGE;
    let _ = tree_sitter_c::LANGUAGE;
    let _ = tree_sitter_cpp::LANGUAGE;
    let _ = tree_sitter_dart::LANGUAGE;
    println!("Compiles!");
}

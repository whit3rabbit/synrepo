use super::super::*;
use std::path::Path;

#[test]
fn parse_file_returns_none_for_unsupported_extension() {
    assert!(parse_file(Path::new("config.yaml"), b"key: val")
        .unwrap()
        .is_none());
    assert!(parse_file(Path::new("README.md"), b"# hi")
        .unwrap()
        .is_none());
}

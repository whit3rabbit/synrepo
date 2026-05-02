//! Shared test-file path convention helpers.

use std::path::Path;

pub(crate) fn matches_path_convention(test_path: &str, source_path: &str) -> bool {
    let source = Path::new(source_path);
    let Some(source_stem) = source.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    let test_name = Path::new(test_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(test_path);

    if test_name.starts_with(&format!("{source_stem}_test"))
        || test_name.starts_with(&format!("test_{source_stem}"))
        || test_name.starts_with(&format!("{source_stem}.test"))
        || test_name.starts_with(&format!("{source_stem}.spec"))
    {
        return true;
    }

    if test_path == format!("tests/{source_stem}.rs")
        || test_path == format!("tests/{source_stem}.py")
    {
        return true;
    }

    let Some(source_dir) = source.parent().and_then(|dir| dir.to_str()) else {
        return false;
    };
    test_path.starts_with(&format!("{source_dir}/tests/"))
        || test_path.starts_with(&format!("{source_dir}/__tests__/"))
}

#[cfg(test)]
mod tests {
    use super::matches_path_convention;

    #[test]
    fn matches_common_language_test_file_conventions() {
        let source = "src/parser/main.rs";

        assert!(matches_path_convention("src/parser/main_test.rs", source));
        assert!(matches_path_convention("src/parser/test_main.py", source));
        assert!(matches_path_convention("src/parser/main.test.ts", source));
        assert!(matches_path_convention("src/parser/main.spec.tsx", source));
        assert!(matches_path_convention("src/parser/main_test.go", source));
        assert!(matches_path_convention("tests/main.rs", source));
        assert!(matches_path_convention("tests/main.py", source));
    }

    #[test]
    fn matches_same_directory_test_folders() {
        let source = "src/parser/main.ts";

        assert!(matches_path_convention(
            "src/parser/tests/any_name.ts",
            source
        ));
        assert!(matches_path_convention(
            "src/parser/__tests__/main_case.ts",
            source
        ));
    }

    #[test]
    fn rejects_unrelated_paths() {
        let source = "src/parser/main.rs";

        assert!(!matches_path_convention("src/parser/domain.rs", source));
        assert!(!matches_path_convention("src/other/tests/main.rs", source));
        assert!(!matches_path_convention("tests/domain.rs", source));
    }
}

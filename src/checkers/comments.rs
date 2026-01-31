//! Comment checker for TODO, FIXME, HACK, and other markers
//!
//! Detects actionable comments that should be addressed.

use crate::checkers::Checker;
use crate::types::{CheckType, Diagnostic, Severity};
use rayon::prelude::*;
use regex::Regex;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// Patterns for detecting actionable comments
static COMMENT_PATTERNS: LazyLock<Vec<CommentPattern>> = LazyLock::new(|| {
    vec![
        CommentPattern {
            regex: Regex::new(r"(?i)\bTODO\b[:\s]*(.*)").unwrap(),
            marker: "TODO",
            severity: Severity::Info,
            message: "TODO comment found",
        },
        CommentPattern {
            regex: Regex::new(r"(?i)\bFIXME\b[:\s]*(.*)").unwrap(),
            marker: "FIXME",
            severity: Severity::Warning,
            message: "FIXME comment found - indicates a bug or issue",
        },
        CommentPattern {
            regex: Regex::new(r"(?i)\bHACK\b[:\s]*(.*)").unwrap(),
            marker: "HACK",
            severity: Severity::Warning,
            message: "HACK comment found - indicates a workaround",
        },
        CommentPattern {
            regex: Regex::new(r"(?i)\bXXX\b[:\s]*(.*)").unwrap(),
            marker: "XXX",
            severity: Severity::Warning,
            message: "XXX comment found - requires attention",
        },
        CommentPattern {
            regex: Regex::new(r"(?i)\bBUG\b[:\s]*(.*)").unwrap(),
            marker: "BUG",
            severity: Severity::Error,
            message: "BUG comment found - known bug marker",
        },
        CommentPattern {
            regex: Regex::new(r"(?i)\bNOTE\b[:\s]*(.*)").unwrap(),
            marker: "NOTE",
            severity: Severity::Info,
            message: "NOTE comment found",
        },
        CommentPattern {
            regex: Regex::new(r"(?i)\b(DEPRECATED|@deprecated)\b[:\s]*(.*)").unwrap(),
            marker: "DEPRECATED",
            severity: Severity::Warning,
            message: "Deprecated code marker found",
        },
    ]
});

struct CommentPattern {
    regex: Regex,
    marker: &'static str,
    severity: Severity,
    message: &'static str,
}

/// Comment checker that finds TODO, FIXME, HACK, etc.
pub struct CommentChecker;

impl Default for CommentChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl CommentChecker {
    pub fn new() -> Self {
        Self
    }

    fn check_file(&self, path: &Path) -> Vec<Diagnostic> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let filename = path.to_string_lossy().to_string();
        let mut diagnostics = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_number = (line_num + 1) as u32;

            // Check for comment markers
            for pattern in COMMENT_PATTERNS.iter() {
                if let Some(captures) = pattern.regex.captures(line) {
                    let description = captures
                        .get(1)
                        .map(|m| m.as_str().trim())
                        .unwrap_or("")
                        .to_string();

                    let column = line
                        .find(pattern.marker)
                        .map(|i| (i + 1) as u32)
                        .unwrap_or(1);

                    let message = if description.is_empty() {
                        pattern.message.to_string()
                    } else {
                        format!("{}: {}", pattern.message, description)
                    };

                    diagnostics.push(Diagnostic {
                        file: filename.clone(),
                        line: line_number,
                        column,
                        message,
                        rule: format!("comment-{}", pattern.marker.to_lowercase()),
                        severity: pattern.severity,
                        suggestion: Some(format!(
                            "Address the {} comment or remove if no longer applicable",
                            pattern.marker
                        )),
                    });
                }
            }
        }

        diagnostics
    }
}

impl Checker for CommentChecker {
    fn check_type(&self) -> CheckType {
        CheckType::Comments
    }

    fn check(&self, repo_path: &Path) -> Vec<Diagnostic> {
        let files = collect_source_files(repo_path);

        if files.is_empty() {
            return vec![];
        }

        files
            .par_iter()
            .flat_map(|file| self.check_file(file))
            .collect()
    }
}

/// Collect all source files from a directory
fn collect_source_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_source_files_recursive(dir, &mut files);
    files
}

fn collect_source_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip common non-source directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
                || name == "vendor"
                || name == "__pycache__"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_source_files_recursive(&path, files);
        } else if is_source_file(&path) {
            files.push(path);
        }
    }
}

fn is_source_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "js" | "jsx"
            | "ts"
            | "tsx"
            | "rs"
            | "py"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "rb"
            | "php"
            | "swift"
            | "kt"
            | "scala"
            | "cs"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_detect_todo() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "// TODO: implement this feature");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "comment-todo");
        assert!(diagnostics[0].message.contains("implement this feature"));
    }

    #[test]
    fn test_detect_fixme() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.rs", "// FIXME: this is broken");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "comment-fixme");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn test_detect_hack() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.py", "# HACK: temporary workaround");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "comment-hack");
    }

    #[test]
    fn test_detect_bug() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "/* BUG: race condition */");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "comment-bug");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn test_multiple_comments() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let content = r#"
// TODO: first task
function foo() {
    // FIXME: fix this
    return 1;
}
// HACK: temporary
"#;
        let path = create_test_file(&dir, "test.js", content);

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 3);
    }

    #[test]
    fn test_case_insensitive() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "// todo: lowercase works too");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "comment-todo");
    }

    #[test]
    fn test_no_comments() {
        let checker = CommentChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "const x = 1;");

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics.is_empty());
    }
}

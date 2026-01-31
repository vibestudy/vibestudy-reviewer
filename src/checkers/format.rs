//! Format checker for code style issues
//!
//! Detects common formatting problems like trailing whitespace,
//! missing newlines, inconsistent indentation, etc.

use crate::checkers::Checker;
use crate::types::{CheckType, Diagnostic, Severity};
use rayon::prelude::*;
use std::fs;
use std::path::Path;

/// Format issues to check for
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatRule {
    /// Trailing whitespace at end of lines
    TrailingWhitespace,
    /// File doesn't end with newline
    MissingFinalNewline,
    /// Mixed tabs and spaces
    MixedIndentation,
    /// Lines exceeding max length
    LineTooLong,
    /// Consecutive blank lines
    MultipleBlankLines,
}

/// Format checker that finds style issues
pub struct FormatChecker {
    max_line_length: usize,
    max_blank_lines: usize,
}

impl Default for FormatChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatChecker {
    pub fn new() -> Self {
        Self {
            max_line_length: 120,
            max_blank_lines: 2,
        }
    }

    /// Create a format checker with custom settings
    pub fn with_settings(max_line_length: usize, max_blank_lines: usize) -> Self {
        Self {
            max_line_length,
            max_blank_lines,
        }
    }

    fn check_file(&self, path: &Path) -> Vec<Diagnostic> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let filename = path.to_string_lossy().to_string();
        let mut diagnostics = Vec::new();

        // Check trailing whitespace and line length
        let mut consecutive_blank_lines = 0;
        let mut has_tabs = false;
        let mut has_spaces = false;

        for (line_num, line) in content.lines().enumerate() {
            let line_number = (line_num + 1) as u32;

            // Trailing whitespace
            if line.ends_with(' ') || line.ends_with('\t') {
                diagnostics.push(Diagnostic {
                    file: filename.clone(),
                    line: line_number,
                    column: line.len() as u32,
                    message: "Trailing whitespace".to_string(),
                    rule: "trailing-whitespace".to_string(),
                    severity: Severity::Info,
                    suggestion: Some("Remove trailing whitespace".to_string()),
                });
            }

            // Line too long
            if line.len() > self.max_line_length {
                diagnostics.push(Diagnostic {
                    file: filename.clone(),
                    line: line_number,
                    column: (self.max_line_length + 1) as u32,
                    message: format!(
                        "Line exceeds {} characters ({} chars)",
                        self.max_line_length,
                        line.len()
                    ),
                    rule: "line-too-long".to_string(),
                    severity: Severity::Info,
                    suggestion: Some("Consider breaking the line".to_string()),
                });
            }

            // Track indentation style
            let leading = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            if leading.contains('\t') {
                has_tabs = true;
            }
            if leading.contains(' ') && !leading.is_empty() {
                has_spaces = true;
            }

            // Consecutive blank lines
            if line.trim().is_empty() {
                consecutive_blank_lines += 1;
                if consecutive_blank_lines > self.max_blank_lines {
                    diagnostics.push(Diagnostic {
                        file: filename.clone(),
                        line: line_number,
                        column: 1,
                        message: format!(
                            "More than {} consecutive blank lines",
                            self.max_blank_lines
                        ),
                        rule: "multiple-blank-lines".to_string(),
                        severity: Severity::Info,
                        suggestion: Some("Remove extra blank lines".to_string()),
                    });
                }
            } else {
                consecutive_blank_lines = 0;
            }
        }

        // Check for mixed indentation
        if has_tabs && has_spaces {
            diagnostics.push(Diagnostic {
                file: filename.clone(),
                line: 1,
                column: 1,
                message: "File uses mixed tabs and spaces for indentation".to_string(),
                rule: "mixed-indentation".to_string(),
                severity: Severity::Warning,
                suggestion: Some(
                    "Use consistent indentation (tabs or spaces, not both)".to_string(),
                ),
            });
        }

        // Check for final newline
        if !content.is_empty() && !content.ends_with('\n') {
            let last_line = content.lines().count() as u32;
            diagnostics.push(Diagnostic {
                file: filename.clone(),
                line: last_line,
                column: 1,
                message: "File should end with a newline".to_string(),
                rule: "missing-final-newline".to_string(),
                severity: Severity::Info,
                suggestion: Some("Add a newline at the end of the file".to_string()),
            });
        }

        diagnostics
    }
}

impl Checker for FormatChecker {
    fn check_type(&self) -> CheckType {
        CheckType::Format
    }

    fn check(&self, repo_path: &Path) -> Vec<Diagnostic> {
        let files = collect_formattable_files(repo_path);

        if files.is_empty() {
            return vec![];
        }

        files
            .par_iter()
            .flat_map(|file| self.check_file(file))
            .collect()
    }
}

/// Collect all formattable files from a directory
fn collect_formattable_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_formattable_files_recursive(dir, &mut files);
    files
}

fn collect_formattable_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
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
            collect_formattable_files_recursive(&path, files);
        } else if is_formattable_file(&path) {
            files.push(path);
        }
    }
}

fn is_formattable_file(path: &Path) -> bool {
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
            | "md"
            | "json"
            | "yaml"
            | "yml"
            | "toml"
            | "css"
            | "scss"
            | "less"
            | "html"
            | "vue"
            | "svelte"
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
    fn test_trailing_whitespace() {
        let checker = FormatChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "const x = 1;   \nconst y = 2;\n");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "trailing-whitespace");
        assert_eq!(diagnostics[0].line, 1);
    }

    #[test]
    fn test_missing_final_newline() {
        let checker = FormatChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "const x = 1;");

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics
            .iter()
            .any(|d| d.rule == "missing-final-newline"));
    }

    #[test]
    fn test_line_too_long() {
        let checker = FormatChecker::with_settings(80, 2);
        let dir = TempDir::new().unwrap();
        let long_line =
            "const x = 'this is a very long line that exceeds the maximum allowed length of 80 characters';\n";
        let path = create_test_file(&dir, "test.js", long_line);

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics.iter().any(|d| d.rule == "line-too-long"));
    }

    #[test]
    fn test_multiple_blank_lines() {
        let checker = FormatChecker::new();
        let dir = TempDir::new().unwrap();
        let content = "const x = 1;\n\n\n\nconst y = 2;\n";
        let path = create_test_file(&dir, "test.js", content);

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics.iter().any(|d| d.rule == "multiple-blank-lines"));
    }

    #[test]
    fn test_mixed_indentation() {
        let checker = FormatChecker::new();
        let dir = TempDir::new().unwrap();
        let content = "\tfunction foo() {\n    return 1;\n}\n";
        let path = create_test_file(&dir, "test.js", content);

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics.iter().any(|d| d.rule == "mixed-indentation"));
    }

    #[test]
    fn test_clean_file() {
        let checker = FormatChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "const x = 1;\nconst y = 2;\n");

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics.is_empty());
    }
}

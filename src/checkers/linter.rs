//! JavaScript/TypeScript linting powered by OXC
//!
//! Fast AST-based linting with customizable rules.

use crate::checkers::Checker;
use crate::types::{CheckType, Diagnostic, Severity};
use oxc_allocator::Allocator;
use oxc_ast::ast::{CallExpression, Expression, VariableDeclarationKind};
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Available lint rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintRule {
    /// Disallow console.* calls
    NoConsole,
    /// Disallow debugger statements
    NoDebugger,
    /// Disallow alert/confirm/prompt
    NoAlert,
    /// Disallow eval()
    NoEval,
    /// Disallow var declarations (prefer let/const)
    NoVar,
    /// Disallow duplicate keys in objects
    NoDuplicateKeys,
}

impl LintRule {
    /// Get recommended rules (good defaults)
    pub fn recommended() -> Vec<LintRule> {
        vec![
            LintRule::NoDebugger,
            LintRule::NoEval,
            LintRule::NoVar,
            LintRule::NoDuplicateKeys,
        ]
    }
}

/// The linter configuration and executor
pub struct Linter {
    rules: HashSet<LintRule>,
}

impl Default for Linter {
    fn default() -> Self {
        Self {
            rules: LintRule::recommended().into_iter().collect(),
        }
    }
}

impl Linter {
    /// Create a new linter with default rules
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a linter with specified rules
    pub fn with_rules(rules: Vec<LintRule>) -> Self {
        Self {
            rules: rules.into_iter().collect(),
        }
    }

    /// Check if a rule is enabled
    pub fn has_rule(&self, rule: LintRule) -> bool {
        self.rules.contains(&rule)
    }

    /// Lint a single file
    fn lint_file(&self, path: &Path) -> Vec<Diagnostic> {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let filename = path.to_string_lossy().to_string();
        self.lint_source(&filename, &source)
    }

    /// Lint source code directly
    fn lint_source(&self, filename: &str, source: &str) -> Vec<Diagnostic> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(filename).unwrap_or_default();

        let parser = Parser::new(&allocator, source, source_type);
        let ret = parser.parse();

        // Return parse errors as diagnostics
        if !ret.errors.is_empty() {
            return ret
                .errors
                .iter()
                .map(|e| Diagnostic {
                    file: filename.to_string(),
                    line: 1,
                    column: 1,
                    message: e.to_string(),
                    rule: "parse-error".to_string(),
                    severity: Severity::Error,
                    suggestion: None,
                })
                .collect();
        }

        let mut visitor = LintVisitor::new(filename.to_string(), source, self);
        visitor.visit_program(&ret.program);

        visitor.diagnostics
    }
}

impl Checker for Linter {
    fn check_type(&self) -> CheckType {
        CheckType::Lint
    }

    fn check(&self, repo_path: &Path) -> Vec<Diagnostic> {
        // Find all JS/TS files
        let files = collect_js_ts_files(repo_path);

        if files.is_empty() {
            return vec![];
        }

        // Lint in parallel
        files
            .par_iter()
            .flat_map(|file| self.lint_file(file))
            .collect()
    }
}

struct LintVisitor<'a> {
    file: String,
    source: &'a str,
    config: &'a Linter,
    diagnostics: Vec<Diagnostic>,
    current_object_keys: Vec<HashSet<String>>,
}

impl<'a> LintVisitor<'a> {
    fn new(file: String, source: &'a str, config: &'a Linter) -> Self {
        Self {
            file,
            source,
            config,
            diagnostics: Vec::new(),
            current_object_keys: Vec::new(),
        }
    }

    fn get_line_col(&self, offset: u32) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 1u32;

        for (i, ch) in self.source.char_indices() {
            if i as u32 >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        (line, col)
    }

    fn add_diagnostic(
        &mut self,
        offset: u32,
        message: &str,
        rule: &str,
        severity: Severity,
        suggestion: Option<&str>,
    ) {
        let (line, column) = self.get_line_col(offset);
        self.diagnostics.push(Diagnostic {
            file: self.file.clone(),
            line,
            column,
            message: message.to_string(),
            rule: rule.to_string(),
            severity,
            suggestion: suggestion.map(|s| s.to_string()),
        });
    }
}

impl<'a> Visit<'a> for LintVisitor<'a> {
    fn visit_debugger_statement(&mut self, stmt: &oxc_ast::ast::DebuggerStatement) {
        if self.config.has_rule(LintRule::NoDebugger) {
            self.add_diagnostic(
                stmt.span.start,
                "Unexpected 'debugger' statement",
                "no-debugger",
                Severity::Error,
                Some("Remove the debugger statement before committing"),
            );
        }
    }

    fn visit_call_expression(&mut self, expr: &CallExpression<'a>) {
        // no-console
        if self.config.has_rule(LintRule::NoConsole) {
            if let Expression::StaticMemberExpression(member) = &expr.callee {
                if let Expression::Identifier(id) = &member.object {
                    if id.name == "console" {
                        self.add_diagnostic(
                            expr.span.start,
                            &format!("Unexpected console.{} call", member.property.name),
                            "no-console",
                            Severity::Warning,
                            Some("Remove console calls or use a proper logging library"),
                        );
                    }
                }
            }
        }

        // no-alert
        if self.config.has_rule(LintRule::NoAlert) {
            if let Expression::Identifier(id) = &expr.callee {
                if matches!(id.name.as_str(), "alert" | "confirm" | "prompt") {
                    self.add_diagnostic(
                        expr.span.start,
                        &format!("Unexpected {}() call", id.name),
                        "no-alert",
                        Severity::Warning,
                        Some("Use a modal or toast library instead"),
                    );
                }
            }
        }

        // no-eval
        if self.config.has_rule(LintRule::NoEval) {
            if let Expression::Identifier(id) = &expr.callee {
                if id.name == "eval" {
                    self.add_diagnostic(
                        expr.span.start,
                        "eval() is a security risk and should be avoided",
                        "no-eval",
                        Severity::Error,
                        Some("Use safer alternatives like JSON.parse() for data"),
                    );
                }
            }
        }

        walk::walk_call_expression(self, expr);
    }

    fn visit_variable_declaration(&mut self, decl: &oxc_ast::ast::VariableDeclaration<'a>) {
        // no-var
        if self.config.has_rule(LintRule::NoVar) && decl.kind == VariableDeclarationKind::Var {
            self.add_diagnostic(
                decl.span.start,
                "Unexpected var, use let or const instead",
                "no-var",
                Severity::Warning,
                Some("Replace 'var' with 'let' or 'const'"),
            );
        }

        walk::walk_variable_declaration(self, decl);
    }

    fn visit_object_expression(&mut self, obj: &oxc_ast::ast::ObjectExpression<'a>) {
        // no-duplicate-keys
        if self.config.has_rule(LintRule::NoDuplicateKeys) {
            let mut keys = HashSet::new();
            for prop in &obj.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
                    if let oxc_ast::ast::PropertyKey::StaticIdentifier(id) = &p.key {
                        let key_name = id.name.to_string();
                        if keys.contains(&key_name) {
                            self.add_diagnostic(
                                p.span.start,
                                &format!("Duplicate key '{}'", key_name),
                                "no-duplicate-keys",
                                Severity::Error,
                                Some("Remove the duplicate key or rename one of them"),
                            );
                        } else {
                            keys.insert(key_name);
                        }
                    }
                }
            }
            self.current_object_keys.push(keys);
        }

        walk::walk_object_expression(self, obj);

        if self.config.has_rule(LintRule::NoDuplicateKeys) {
            self.current_object_keys.pop();
        }
    }
}

/// Collect all JS/TS files from a directory
fn collect_js_ts_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_js_ts_files_recursive(dir, &mut files);
    files
}

fn collect_js_ts_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip node_modules and hidden directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "build"
            {
                continue;
            }
        }

        if path.is_dir() {
            collect_js_ts_files_recursive(&path, files);
        } else if is_js_ts_file(&path) {
            files.push(path);
        }
    }
}

fn is_js_ts_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_debugger() {
        let linter = Linter::with_rules(vec![LintRule::NoDebugger]);
        let diagnostics = linter.lint_source("test.js", "debugger;");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-debugger");
    }

    #[test]
    fn test_no_var() {
        let linter = Linter::with_rules(vec![LintRule::NoVar]);
        let diagnostics = linter.lint_source("test.js", "var x = 1;");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-var");
    }

    #[test]
    fn test_no_eval() {
        let linter = Linter::with_rules(vec![LintRule::NoEval]);
        let diagnostics = linter.lint_source("test.js", r#"eval("x")"#);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-eval");
    }

    #[test]
    fn test_no_duplicate_keys() {
        let linter = Linter::with_rules(vec![LintRule::NoDuplicateKeys]);
        let diagnostics = linter.lint_source("test.js", "const obj = { a: 1, a: 2 };");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "no-duplicate-keys");
    }

    #[test]
    fn test_clean_code() {
        let linter = Linter::new();
        let diagnostics = linter.lint_source("test.js", "const x = 1;");

        assert!(diagnostics.is_empty());
    }
}

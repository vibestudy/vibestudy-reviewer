pub mod comments;
pub mod format;
pub mod linter;
pub mod typos;

use crate::types::{CheckType, Diagnostic};
use std::path::Path;

pub trait Checker: Send + Sync {
    fn check_type(&self) -> CheckType;
    fn check(&self, repo_path: &Path) -> Vec<Diagnostic>;
}

pub fn run_all_checkers(repo_path: &Path) -> Vec<(CheckType, Vec<Diagnostic>)> {
    let checkers: Vec<Box<dyn Checker>> = vec![
        Box::new(linter::Linter::new()),
        Box::new(comments::CommentChecker::new()),
        Box::new(typos::TyposChecker::new()),
        Box::new(format::FormatChecker::new()),
    ];

    checkers
        .into_iter()
        .map(|checker| {
            let check_type = checker.check_type();
            let diagnostics = checker.check(repo_path);
            (check_type, diagnostics)
        })
        .collect()
}

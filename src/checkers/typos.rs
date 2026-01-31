//! Typo detection for common spelling mistakes

use crate::checkers::Checker;
use crate::types::{CheckType, Diagnostic, Severity};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

static COMMON_TYPOS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("teh", "the"),
        ("adn", "and"),
        ("taht", "that"),
        ("hte", "the"),
        ("wiht", "with"),
        ("thnig", "thing"),
        ("thigns", "things"),
        ("funciton", "function"),
        ("fucntion", "function"),
        ("funtion", "function"),
        ("retrun", "return"),
        ("reutrn", "return"),
        ("retrn", "return"),
        ("calss", "class"),
        ("classs", "class"),
        ("improt", "import"),
        ("imoprt", "import"),
        ("exoprt", "export"),
        ("exprot", "export"),
        ("cosnt", "const"),
        ("conts", "const"),
        ("varaible", "variable"),
        ("variabel", "variable"),
        ("varible", "variable"),
        ("strign", "string"),
        ("stirng", "string"),
        ("nubmer", "number"),
        ("numebr", "number"),
        ("booelan", "boolean"),
        ("bolean", "boolean"),
        ("arrary", "array"),
        ("arrray", "array"),
        ("obejct", "object"),
        ("objetc", "object"),
        ("objcet", "object"),
        ("lenght", "length"),
        ("legnth", "length"),
        ("widht", "width"),
        ("heigth", "height"),
        ("hieght", "height"),
        ("recieve", "receive"),
        ("recive", "receive"),
        ("occured", "occurred"),
        ("occuring", "occurring"),
        ("seperate", "separate"),
        ("seperator", "separator"),
        ("definately", "definitely"),
        ("defintely", "definitely"),
        ("neccessary", "necessary"),
        ("necesary", "necessary"),
        ("occassion", "occasion"),
        ("occurence", "occurrence"),
        ("adress", "address"),
        ("addresss", "address"),
        ("enviroment", "environment"),
        ("enviornment", "environment"),
        ("refrence", "reference"),
        ("referece", "reference"),
        ("langauge", "language"),
        ("languge", "language"),
        ("paramter", "parameter"),
        ("paramater", "parameter"),
        ("arguement", "argument"),
        ("arguemnt", "argument"),
        ("initalize", "initialize"),
        ("intialize", "initialize"),
        ("implment", "implement"),
        ("implemenation", "implementation"),
        ("responce", "response"),
        ("reponse", "response"),
        ("requried", "required"),
        ("requred", "required"),
        ("availible", "available"),
        ("avialable", "available"),
        ("visable", "visible"),
        ("visiable", "visible"),
        ("specifiy", "specify"),
        ("specifc", "specific"),
        ("acccess", "access"),
        ("acces", "access"),
        ("successfull", "successful"),
        ("succesful", "successful"),
        ("becuase", "because"),
        ("beacuse", "because"),
        ("differnt", "different"),
        ("diffrent", "different"),
        ("similiar", "similar"),
        ("simlar", "similar"),
        ("containts", "contains"),
        ("contians", "contains"),
        ("incldue", "include"),
        ("inculde", "include"),
        ("defualt", "default"),
        ("deafult", "default"),
        ("mesage", "message"),
        ("messsage", "message"),
        ("messgae", "message"),
        ("reuslt", "result"),
        ("resutl", "result"),
        ("reslut", "result"),
    ])
});

pub struct TyposChecker;

impl Default for TyposChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TyposChecker {
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

            for word in extract_words(line) {
                let lower = word.text.to_lowercase();
                if let Some(&correction) = COMMON_TYPOS.get(lower.as_str()) {
                    diagnostics.push(Diagnostic {
                        file: filename.clone(),
                        line: line_number,
                        column: (word.start + 1) as u32,
                        message: format!("Possible typo: '{}' -> '{}'", word.text, correction),
                        rule: "typo".to_string(),
                        severity: Severity::Info,
                        suggestion: Some(format!("Did you mean '{}'?", correction)),
                    });
                }
            }
        }

        diagnostics
    }
}

struct Word<'a> {
    text: &'a str,
    start: usize,
}

fn extract_words(line: &str) -> Vec<Word<'_>> {
    let mut words = Vec::new();
    let mut start = None;

    for (i, c) in line.char_indices() {
        if c.is_alphabetic() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start {
            let word = &line[s..i];
            if word.len() >= 3 {
                words.push(Word {
                    text: word,
                    start: s,
                });
            }
            start = None;
        }
    }

    if let Some(s) = start {
        let word = &line[s..];
        if word.len() >= 3 {
            words.push(Word {
                text: word,
                start: s,
            });
        }
    }

    words
}

impl Checker for TyposChecker {
    fn check_type(&self) -> CheckType {
        CheckType::Typos
    }

    fn check(&self, repo_path: &Path) -> Vec<Diagnostic> {
        let files = collect_checkable_files(repo_path);

        if files.is_empty() {
            return vec![];
        }

        files
            .par_iter()
            .flat_map(|file| self.check_file(file))
            .collect()
    }
}

fn collect_checkable_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_checkable_files_recursive(dir, &mut files);
    files
}

fn collect_checkable_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

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
            collect_checkable_files_recursive(&path, files);
        } else if is_checkable_file(&path) {
            files.push(path);
        }
    }
}

fn is_checkable_file(path: &Path) -> bool {
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
            | "txt"
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
    fn test_detect_typo() {
        let checker = TyposChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.txt", "teh quick brown fox");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "typo");
        assert!(diagnostics[0].message.contains("the"));
    }

    #[test]
    fn test_detect_function_typo() {
        let checker = TyposChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.js", "funciton foo() { retrun 1; }");

        let diagnostics = checker.check_file(&path);

        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_no_typos() {
        let checker = TyposChecker::new();
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.txt", "the quick brown fox");

        let diagnostics = checker.check_file(&path);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_extract_words() {
        let words = extract_words("const foo = 'bar'");
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].text, "const");
        assert_eq!(words[1].text, "foo");
        assert_eq!(words[2].text, "bar");
    }
}

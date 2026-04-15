use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files_under(root: &Path, relative_dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let start = root.join(relative_dir);
    let mut stack = vec![start];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn extract_string_violations(regex: &Regex, source: &str, relative_path: &str) -> BTreeSet<String> {
    regex
        .captures_iter(source)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().trim_start()))
        .filter(|message| {
            message
                .chars()
                .find(|ch| ch.is_ascii_alphabetic())
                .is_some_and(|ch| ch.is_ascii_uppercase())
        })
        .map(|message| format!("{relative_path}::{message}"))
        .collect()
}

fn current_violations() -> BTreeSet<String> {
    let root = repo_root();
    let regexes = [
        Regex::new(r#"(?s)\b(?:eyre|bail)!\(\s*"([^"]+)""#).unwrap(),
        Regex::new(r#"(?s)\b(?:eyre|bail)!\(\s*format!\(\s*"([^"]+)""#).unwrap(),
        Regex::new(r#"(?s)\.wrap_err\(\s*"([^"]+)""#).unwrap(),
        Regex::new(r#"(?s)\.wrap_err\(\s*format!\(\s*"([^"]+)""#).unwrap(),
        Regex::new(r#"(?s)\.wrap_err_with\(\s*\|\|[^;]*?"([^"]+)""#).unwrap(),
        Regex::new(r#"(?s)\bensure!\([^;]*?,\s*"([^"]+)""#).unwrap(),
        Regex::new(r#"(?s)\bensure!\([^;]*?,\s*format!\(\s*"([^"]+)""#).unwrap(),
    ];

    let mut violations = BTreeSet::new();
    for path in rust_files_under(&root, "src")
        .into_iter()
        .chain(rust_files_under(&root, "crates"))
    {
        let source = fs::read_to_string(&path).unwrap();
        let relative_path = path.strip_prefix(&root).unwrap().display().to_string();
        for regex in &regexes {
            violations.extend(extract_string_violations(regex, &source, &relative_path));
        }
    }
    violations
}

fn legacy_allowlist() -> BTreeSet<String> {
    [].into_iter().map(str::to_string).collect()
}

#[test]
fn error_messages_must_start_with_lowercase() {
    let violations = current_violations();
    let allowlist = legacy_allowlist();
    let unexpected = violations
        .difference(&allowlist)
        .cloned()
        .collect::<Vec<_>>();
    let stale = allowlist
        .difference(&violations)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "error message style baseline changed\nunexpected:\n{}\nstale:\n{}",
        if unexpected.is_empty() {
            "<none>".to_string()
        } else {
            unexpected.join("\n")
        },
        if stale.is_empty() {
            "<none>".to_string()
        } else {
            stale.join("\n")
        }
    );
}

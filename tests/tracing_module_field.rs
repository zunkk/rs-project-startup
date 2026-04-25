use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const LOG_MACROS: &[&str] = &["trace", "debug", "info", "warn", "error", "event"];

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
                if path
                    .components()
                    .any(|component| component.as_os_str() == "tests")
                {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn current_violations() -> BTreeSet<String> {
    let root = repo_root();
    let mut violations = BTreeSet::new();
    for path in rust_files_under(&root, "src")
        .into_iter()
        .chain(rust_files_under(&root, "crates"))
    {
        let source = fs::read_to_string(&path).unwrap();
        let relative_path = path.strip_prefix(&root).unwrap().display().to_string();
        violations.extend(extract_module_violations(
            &strip_cfg_test_modules(&source),
            &relative_path,
        ));
    }
    violations
}

fn extract_module_violations(source: &str, relative_path: &str) -> BTreeSet<String> {
    let mut violations = BTreeSet::new();
    let module_log_macros = module_log_macro_imports(source);
    let mut offset = 0;

    while offset < source.len() {
        let Some(relative) = find_tracing_macro(source, offset) else {
            break;
        };
        let macro_start = offset + relative;
        let macro_name_end = macro_start + macro_name_len(source, macro_start);
        let macro_name = &source[macro_start..macro_name_end];
        let open_paren = skip_whitespace(source, macro_name_end + 1);
        let Some(close_paren) = find_matching_delimiter(source, open_paren, b'(', b')') else {
            offset = macro_start + 1;
            continue;
        };
        let body = &source[open_paren + 1..close_paren];
        if !has_module_field(body) && !module_log_macros.contains(macro_name) {
            let line = source[..macro_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            let first_line = source[macro_start..close_paren]
                .lines()
                .next()
                .unwrap_or_default()
                .trim();
            violations.insert(format!("{relative_path}:{line}: {first_line}"));
        }
        offset = close_paren + 1;
    }

    violations
}

fn module_log_macro_imports(source: &str) -> BTreeSet<String> {
    let mut imports = BTreeSet::new();
    if source.contains("define_module_log_macros!") {
        imports.extend(LOG_MACROS.iter().map(|name| name.to_string()));
    }
    let mut offset = 0;

    while let Some(relative) = source[offset..].find("use ") {
        let start = offset + relative;
        let Some(statement_end) = source[start..].find(';').map(|index| start + index) else {
            break;
        };
        let statement = &source[start..statement_end];
        collect_module_log_macro_imports(statement, &mut imports);
        offset = statement_end + 1;
    }

    imports
}

fn collect_module_log_macro_imports(statement: &str, imports: &mut BTreeSet<String>) {
    let trimmed = statement.trim();
    let imports_from_log_module = trimmed.contains("::log::")
        || trimmed.contains("::log::{")
        || trimmed.starts_with("use super::")
        || trimmed.starts_with("use self::");
    if !imports_from_log_module {
        return;
    }

    if let Some(group_start) = trimmed.find('{') {
        let Some(group_end) = trimmed.rfind('}') else {
            return;
        };
        for item in trimmed[group_start + 1..group_end].split(',') {
            let name = item.trim();
            if LOG_MACROS.contains(&name) {
                imports.insert(name.to_string());
            }
        }
        return;
    }

    let Some(name) = trimmed.rsplit("::").next() else {
        return;
    };
    if LOG_MACROS.contains(&name) {
        imports.insert(name.to_string());
    }
}

fn strip_cfg_test_modules(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut offset = 0;

    while let Some(relative) = source[offset..].find("#[cfg(test)]") {
        let attr_start = offset + relative;
        stripped.push_str(&source[offset..attr_start]);

        let mut index = skip_whitespace(source, attr_start + "#[cfg(test)]".len());
        if !starts_with_ident(source, index, "mod") {
            stripped.push_str("#[cfg(test)]");
            offset = attr_start + "#[cfg(test)]".len();
            continue;
        }

        index = skip_whitespace(source, index + "mod".len());
        index += source[index..]
            .bytes()
            .take_while(|byte| is_ident_continue(*byte))
            .count();
        index = skip_whitespace(source, index);

        if source.as_bytes().get(index) != Some(&b'{') {
            stripped.push_str("#[cfg(test)]");
            offset = attr_start + "#[cfg(test)]".len();
            continue;
        }

        let Some(close_brace) = find_matching_delimiter(source, index, b'{', b'}') else {
            let removed = &source[attr_start..];
            stripped.extend(removed.bytes().filter(|byte| *byte == b'\n').map(|_| '\n'));
            return stripped;
        };
        let removed = &source[attr_start..=close_brace];
        stripped.extend(removed.bytes().filter(|byte| *byte == b'\n').map(|_| '\n'));
        offset = close_brace + 1;
    }

    stripped.push_str(&source[offset..]);
    stripped
}

fn find_tracing_macro(source: &str, start: usize) -> Option<usize> {
    let mut index = start;

    while index < source.len() {
        let byte = source.as_bytes()[index];
        if !is_ident_start(byte) {
            index += 1;
            continue;
        }

        let ident_start = index;
        index += 1;
        while index < source.len() && is_ident_continue(source.as_bytes()[index]) {
            index += 1;
        }

        let ident = &source[ident_start..index];
        if !LOG_MACROS.contains(&ident) {
            continue;
        }

        let bang = skip_whitespace(source, index);
        if source.as_bytes().get(bang) != Some(&b'!') {
            continue;
        }

        let open_paren = skip_whitespace(source, bang + 1);
        if source.as_bytes().get(open_paren) != Some(&b'(') {
            continue;
        }

        return Some(ident_start - start);
    }

    None
}

fn has_module_field(body: &str) -> bool {
    split_top_level_args(body)
        .into_iter()
        .any(|(_, arg)| starts_with_field_assignment(arg.trim_start(), "module"))
}

fn starts_with_field_assignment(arg: &str, name: &str) -> bool {
    let Some(rest) = arg.strip_prefix(name) else {
        return false;
    };
    rest.trim_start().starts_with('=')
}

fn split_top_level_args(body: &str) -> Vec<(usize, &str)> {
    let mut args = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;

    while index < body.len() {
        match body.as_bytes()[index] {
            b'"' => {
                index = skip_quoted_string(body, index);
            }
            b'\'' => {
                index = skip_quoted_char(body, index);
            }
            b'(' => {
                paren_depth += 1;
                index += 1;
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                index += 1;
            }
            b'[' => {
                bracket_depth += 1;
                index += 1;
            }
            b']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                index += 1;
            }
            b'{' => {
                brace_depth += 1;
                index += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                index += 1;
            }
            b',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                args.push((start, body[start..index].trim()));
                index += 1;
                start = index;
            }
            _ => {
                index += 1;
            }
        }
    }

    let tail = body[start..].trim();
    if !tail.is_empty() {
        args.push((start, tail));
    }

    args
}

fn find_matching_delimiter(
    source: &str,
    open_delimiter: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open_delimiter;

    while index < source.len() {
        let byte = source.as_bytes()[index];
        if byte == b'"' {
            index = skip_quoted_string(source, index);
        } else if byte == b'\'' {
            index = skip_quoted_char(source, index);
        } else if byte == open {
            depth += 1;
            index += 1;
        } else if byte == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
            index += 1;
        } else {
            index += 1;
        }
    }

    None
}

fn skip_whitespace(source: &str, mut index: usize) -> usize {
    while index < source.len() && source.as_bytes()[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn skip_quoted_string(source: &str, mut index: usize) -> usize {
    index += 1;
    let mut escaped = false;
    while index < source.len() {
        match source.as_bytes()[index] {
            b'\\' if !escaped => {
                escaped = true;
                index += 1;
            }
            b'"' if !escaped => {
                return index + 1;
            }
            _ => {
                escaped = false;
                index += 1;
            }
        }
    }
    source.len()
}

fn skip_quoted_char(source: &str, mut index: usize) -> usize {
    index += 1;
    let mut escaped = false;
    while index < source.len() {
        match source.as_bytes()[index] {
            b'\\' if !escaped => {
                escaped = true;
                index += 1;
            }
            b'\'' if !escaped => {
                return index + 1;
            }
            _ => {
                escaped = false;
                index += 1;
            }
        }
    }
    source.len()
}

fn starts_with_ident(source: &str, index: usize, ident: &str) -> bool {
    source[index..].starts_with(ident)
        && source
            .as_bytes()
            .get(index + ident.len())
            .is_none_or(|byte| !is_ident_continue(*byte))
}

fn macro_name_len(source: &str, macro_start: usize) -> usize {
    source[macro_start..]
        .bytes()
        .take_while(|byte| is_ident_continue(*byte))
        .count()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn legacy_allowlist() -> BTreeSet<String> {
    [].into_iter().map(str::to_string).collect()
}

#[test]
fn tracing_log_macros_must_include_module_field() {
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
        "tracing module field baseline changed\nunexpected:\n{}\nstale:\n{}",
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

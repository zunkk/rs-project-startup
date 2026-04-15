use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

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

fn current_violations() -> BTreeSet<String> {
    let root = repo_root();
    let mut violations = BTreeSet::new();
    for path in rust_files_under(&root, "src")
        .into_iter()
        .chain(rust_files_under(&root, "crates"))
    {
        let source = fs::read_to_string(&path).unwrap();
        let relative_path = path.strip_prefix(&root).unwrap().display().to_string();
        violations.extend(extract_message_violations(&source, &relative_path));
    }
    violations
}

fn extract_message_violations(source: &str, relative_path: &str) -> BTreeSet<String> {
    tracing_message_literals(source)
        .into_iter()
        .filter(|(_, message)| {
            message
                .chars()
                .find(|ch| ch.is_ascii_alphabetic())
                .is_some_and(|ch| ch.is_ascii_lowercase())
        })
        .map(|(offset, message)| {
            let line = source[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            format!("{relative_path}:{line}::{message}")
        })
        .collect()
}

fn tracing_message_literals(source: &str) -> Vec<(usize, String)> {
    let mut messages = Vec::new();
    let mut offset = 0;

    while offset < source.len() {
        let Some(relative) = find_tracing_macro(source, offset) else {
            break;
        };
        let macro_start = offset + relative;
        let open_paren = skip_inline_whitespace(
            source,
            macro_start + macro_name_len(source, macro_start) + 1,
        );
        let Some(close_paren) = find_matching_paren(source, open_paren) else {
            offset = macro_start + 1;
            continue;
        };
        let body = &source[open_paren + 1..close_paren];
        if let Some((body_offset, message)) = first_message_literal(body) {
            messages.push((open_paren + 1 + body_offset, message.to_string()));
        }
        offset = close_paren + 1;
    }

    messages
}

fn find_tracing_macro(source: &str, start: usize) -> Option<usize> {
    let mut index = start;

    while index < source.len() {
        let ch = source.as_bytes()[index];
        if !is_ident_start(ch) {
            index += 1;
            continue;
        }

        let ident_start = index;
        index += 1;
        while index < source.len() && is_ident_continue(source.as_bytes()[index]) {
            index += 1;
        }

        let ident = &source[ident_start..index];
        if !matches!(ident, "trace" | "debug" | "info" | "warn" | "error") {
            continue;
        }

        let bang = skip_inline_whitespace(source, index);
        if source.as_bytes().get(bang) != Some(&b'!') {
            continue;
        }

        let open_paren = skip_inline_whitespace(source, bang + 1);
        if source.as_bytes().get(open_paren) != Some(&b'(') {
            continue;
        }

        return Some(ident_start - start);
    }

    None
}

fn macro_name_len(source: &str, macro_start: usize) -> usize {
    source[macro_start..]
        .bytes()
        .take_while(|byte| is_ident_continue(*byte))
        .count()
}

fn first_message_literal(body: &str) -> Option<(usize, &str)> {
    split_top_level_args(body)
        .into_iter()
        .find_map(|(offset, arg)| {
            let trimmed = arg.trim_start();
            let trimmed_offset = offset + arg.len() - trimmed.len();
            parse_string_literal(trimmed).map(|message| (trimmed_offset, message))
        })
}

fn split_top_level_args(body: &str) -> Vec<(usize, &str)> {
    let mut args = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;

    while index < body.len() {
        let byte = body.as_bytes()[index];
        match byte {
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

fn parse_string_literal(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }

    let mut index = 1;
    let mut escaped = false;
    while index < input.len() {
        match bytes[index] {
            b'\\' if !escaped => {
                escaped = true;
                index += 1;
            }
            b'"' if !escaped => {
                return Some(&input[1..index]);
            }
            _ => {
                escaped = false;
                index += 1;
            }
        }
    }

    None
}

fn find_matching_paren(source: &str, open_paren: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open_paren;

    while index < source.len() {
        let byte = source.as_bytes()[index];
        match byte {
            b'"' => {
                index = skip_quoted_string(source, index);
            }
            b'\'' => {
                index = skip_quoted_char(source, index);
            }
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }

    None
}

fn skip_inline_whitespace(source: &str, mut index: usize) -> usize {
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
fn log_messages_must_start_with_uppercase() {
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
        "log message style baseline changed\nunexpected:\n{}\nstale:\n{}",
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

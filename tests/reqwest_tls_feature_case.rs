use std::fs;

const ROOT_CARGO_TOML: &str = "Cargo.toml";
const TLS_FEATURES: &[&str] = &[
    "default-tls",
    "rustls",
    "native-tls",
    "native-tls-vendored",
    "__rustls",
];

#[test]
fn workspace_reqwest_dependency_should_keep_a_tls_backend() {
    let cargo_toml = fs::read_to_string(ROOT_CARGO_TOML).expect("failed to read Cargo.toml");

    // Find all reqwest = { ... } lines (both in [dependencies] and [workspace.dependencies])
    let reqwest_lines: Vec<&str> = cargo_toml
        .lines()
        .filter(|line| line.trim_start().starts_with("reqwest = {"))
        .collect();

    let has_tls_feature = reqwest_lines
        .iter()
        .any(|line| TLS_FEATURES.iter().any(|feature| line.contains(feature)));

    assert!(
        has_tls_feature,
        "workspace reqwest dependency must keep an explicit tls backend feature, found: {:?}",
        reqwest_lines
    );
}

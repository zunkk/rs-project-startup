use std::fs;

const ROOT_CARGO_TOML: &str = "Cargo.toml";
const RUNTIME_FEATURES: &[&str] = &[
    "runtime-tokio-rustls",
    "runtime-tokio-native-tls",
    "runtime-tokio",
    "runtime-async-std-rustls",
    "runtime-async-std-native-tls",
    "runtime-async-std",
];

#[test]
fn workspace_sea_orm_dependency_should_keep_a_runtime_feature() {
    let cargo_toml = fs::read_to_string(ROOT_CARGO_TOML).expect("failed to read Cargo.toml");

    let sea_orm_lines: Vec<&str> = cargo_toml
        .lines()
        .filter(|line| line.trim_start().starts_with("sea-orm = {"))
        .collect();

    let has_runtime_feature = sea_orm_lines.iter().any(|line| {
        RUNTIME_FEATURES
            .iter()
            .any(|feature| line.contains(feature))
    });

    assert!(
        has_runtime_feature,
        "workspace sea-orm dependency must keep an explicit runtime feature, found: {:?}",
        sea_orm_lines
    );
}

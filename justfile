app-name := "rs-project-startup"
app-description := "A framework for quickly starting a Rust project"
app-version := env_var_or_default('app_version', 'dev')
app-name-underscore := replace(replace(app-name, "-", "_"), " ", "_")

default:
    @just --list --unsorted

init-project-from-template:
    @sed -i '' 's/^name = "rs-project-startup"/name = "{{ app-name }}"/' Cargo.toml
    @sed -i '' 's/^description = "A framework for quickly starting a Rust project"/description = "{{ app-description }}"/' Cargo.toml
    @sed -i '' '/^authors *=/d' Cargo.toml
    @sed -i '' '/^homepage *=/d' Cargo.toml
    @sed -i '' '/^repository *=/d' Cargo.toml
    @sed -i '' '/^license *=/d' Cargo.toml
    @sed -i '' '/^members = \["crates\/\*"\]/d' Cargo.toml
    @sed -i '' 's|^sidecar = { path = "crates/sidecar" }|sidecar = { git = "https://github.com/zunkk/rs-project-startup.git", package = "sidecar", branch = "main" }|' Cargo.toml
    @sed -i '' "s/rs_project_startup/{{ app-name-underscore }}/g" src/bin/export_openapi.rs

init:
    @brew install openapi-generator

fmt:
    @cargo +nightly  fmt --all

clippy:
    @cargo +nightly clippy --fix --all --all-features --allow-staged --allow-dirty

fix:
    @cargo +nightly fix --allow-staged --allow-no-vcs --workspace

opt-code: fix fmt

check:
    @cargo check --workspace

generate-openapi-client:
    @cargo run --bin export_openapi
    @rm -rf target/openapi-client
    @openapi-generator generate \
      -i openapi.json \
      -g rust \
      -o target/openapi-client \
      --additional-properties=library=reqwest,supportAsync=true,supportMiddleware=true,useSingleRequestParameter=true
    @rm -rf src/api/http/client/apis
    @cp -r target/openapi-client/src/apis src/api/http/client/apis
    @rm -rf src/api/http/client/models
    @cp -r target/openapi-client/src/models src/api/http/client/models
    @rm -rf target/openapi-client
    @find src/api/http/client/models -type f -name '*.rs' -exec sh -c 'for file in "$@"; do sed -i "" -e "s/use crate::models;/use super::super::models;/g" "$file"; done' sh {} +
    @find src/api/http/client/apis -type f -name '*_api.rs' -exec sed -i '' \
      -e 's/use crate::{apis::ResponseContent, models};/use super::super::models;/' \
      -e 's/use super::{Error, configuration, ContentType};/use super::{configuration, ContentType, Error, ResponseContent};/' {} +

build:
    APP_VERSION={{ app-version }} cargo build
    @cp target/debug/{{ app-name }} ./

release:
    APP_VERSION={{ app-version }} cargo build --release
    @cp target/release/{{ app-name }} ./

package: release
    rm -f ./deploy/tools/bin/app
    mv ./{{ app-name }} ./deploy/tools/bin/app
    tar czvf ./{{ app-name }}-{{ app-version }}.tar.gz -C ./deploy/ .

package-debug: build
    rm -f ./deploy/tools/bin/app
    mv ./{{ app-name }} ./deploy/tools/bin/app

package-release: release
    rm -f ./deploy/tools/bin/app
    mv ./{{ app-name }} ./deploy/tools/bin/app

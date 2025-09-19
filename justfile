app-name := "rs-project-startup"

default:
    @just --list

init:
    @brew install openapi-generator

fmt:
    @cargo +nightly  fmt --all

clippy:
    @cargo +nightly clippy --all --all-features --allow-staged --allow-dirty --fix

fix:
    @cargo +nightly fix --allow-staged --allow-no-vcs

opt-code: fmt fix

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
    @cargo build
    @cp target/debug/{{ app-name }} ./

release:
    @cargo build --release
    @cp target/release/{{ app-name }} ./

install:
    @cargo install --path .

package-debug: build
    rm -f ./deploy/tools/bin/app
    cp ./{{ app-name }} ./deploy/tools/bin/app

package-release: release
    rm -f ./deploy/tools/bin/app
    cp ./{{ app-name }} ./deploy/tools/bin/app

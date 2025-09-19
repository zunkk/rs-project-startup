# rs-project-startup

## Overview
rs-project-startup is a scaffold for rapidly bootstrapping Rust back-end services. It ships with a CLI entry point, an Axum-based HTTP API, and a reusable Sidecar infrastructure crate, making it a solid starting point for internal services or medium-sized products.

## Using This Repository as a Template
1. Open the root `justfile` and set `app-name` and `app-description` to the target project values.
2. Run `just init-project-from-template`. The recipe will:
   - Update `Cargo.toml` with the new package name and description, and remove authors, homepage, repository, license, and workspace member metadata;
   - Point the `sidecar` dependency to the official template Git repository;
   - Replace the `rs_project_startup` namespace in `src/bin/export_openapi.rs` with the underscored version of the new app name.
3. Add project-specific documentation, licensing, CI configuration, and any additional metadata you require.

## Just Recipe Highlights
- `just init-project-from-template`: Execute the template initialization flow described above.
- `just fmt` / `just clippy` / `just fix`: Format, lint, and automatically apply fixes using the nightly toolchain.
- `just build` / `just release`: Compile in debug or release mode and copy the binary to the repository root for quick inspection.
- `just package` / `just package-debug` / `just package-release`: Produce deployable archives and sync binaries to `deploy/tools/bin`.
- `just generate-openapi-client`: Export the latest OpenAPI spec and regenerate the Rust client under `src/api/http/client`.
- `just opt-code`: Convenience target that runs `fmt` followed by `fix` to tidy the codebase before committing.
- `just init`: Install required local dependencies such as the OpenAPI generator.

## Command Tips
- Override the default version by exporting `app_version`, for example `app_version=0.2.0 just release`.
- For quick experiments you can invoke `cargo` directly, then return to the curated `just` flow to keep artifacts and automation consistent.

## Architecture Overview
The binary entry point (`src/main.rs`) drives the application by delegating CLI commands to the `cmd` module, which then routes work into the service core:
- `src/core/`: Domain models, services, and database access, organized for clear boundaries and testability.
- `src/api/http/`: Axum HTTP server, OpenAPI definitions, and generated client code.
- `src/kit/`: Shared utilities such as configuration loading, context management, JWT helpers, and API response helpers.

### Architectural Characteristics
- **Layered design**: CLI, interface, core, and infrastructure layers remain decoupled for easier extension and testing.
- **Workspace dependency governance**: `workspace.dependencies` centralizes version management and works hand-in-hand with the template initialization script.
- **OpenAPI-driven workflow**: Built-in export and client generation streamline collaboration with frontend teams and SDK maintenance.


# Repository Guidelines

## Project Structure & Module Organization
`src/main.rs` 是 CLI entry，`src/cmd` 放 `config`、`run`、`ipc` 等命令入口。`src/api/http` 负责 HTTP server、OpenAPI 和 generated client；`src/core` 放 DB、model、service；`src/kit` 放 config、JWT、error、response 等通用基础设施。`crates/sidecar` 是可复用的 lifecycle/log/repo crate。`deploy/` 保存打包和运维脚本，`tests/` 主要是仓库级约束测试。修改 API schema 后，同时检查 `src/bin/export_openapi.rs` 与 `src/api/http/client/`。

## Build, Test, and Development Commands
优先使用 `just` 配方：

- `just check`: 运行 `cargo check --workspace`
- `just test`: 运行全部测试
- `just fmt`: 使用 nightly `rustfmt` 格式化
- `just clippy`: 使用 nightly `clippy --fix`
- `just opt-code`: 串行执行 `fix + fmt + clippy`
- `just build` / `just release`: 生成 debug/release binary
- `just generate-openapi-client`: 导出 `openapi.json` 并刷新 Rust client

本地运行前通常先执行 `cargo build`，再用 `./target/debug/rs-project-startup --repo-root . config generate-default` 生成配置。

## Coding Style & Naming Conventions
使用 Rust 2024 edition，遵循 `rustfmt.toml`。导入分组由 `group_imports = "StdExternalCrate"` 控制。模块、文件、函数使用 `snake_case`，类型使用 `CamelCase`，CLI subcommand 名称保持简短直观。不要手改 generated client。仓库有额外风格约束：日志 message 必须以大写字母开头，error 字符串必须以小写字母开头。

## Testing Guidelines
测试通过 `cargo test --workspace` 运行。`tests/` 当前包含风格和依赖守卫测试，例如 `log_message_style.rs`、`error_message_style.rs`、`reqwest_tls_feature_case.rs`。新增测试文件建议按 `<subject>_<expectation>.rs` 命名。涉及 HTTP、IPC、配置或依赖特性变更时，先补守卫测试，再改实现。

## Commit & Pull Request Guidelines
Git 历史当前采用简洁的 Conventional Commits 风格，例如 `feat: opt dependencies`、`feat: update dependencies`。建议继续使用 `feat:`、`fix:`、`refactor:`、`test:`。PR 应说明变更动机、影响模块、验证命令；如果改动 HTTP API、OpenAPI 或部署脚本，附上生成步骤、示例命令或关键输出。

## Configuration & Runtime Notes
运行目录默认是 repo root，常见运行时文件包括 `config.toml`、`.env`、`logs/`、`process.pid`、`ipc.sock`。主进程启动通常依赖 PostgreSQL；仅修改模板代码不足以保证 `run` 可直接成功。发布相关改动请同步检查 `deploy/start.sh`、`deploy/stop.sh` 和 `deploy/update_binary.sh`。

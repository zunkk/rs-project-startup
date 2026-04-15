# rs-project-startup

一个面向 Rust service/backend 场景的启动模板，内置了 `CLI + config + component lifecycle + HTTP API + IPC + OpenAPI + PostgreSQL + deploy script` 的基础骨架。它不是只放一个 `main.rs` 的空仓库，而是已经把服务化项目最常见的几条主链路先搭好了，适合在此基础上继续做业务开发。

当前仓库已经包含：

- `axum` HTTP server 与 `Swagger UI`
- 基于 Unix socket 的 `IPC` 调用链
- `SeaORM + PostgreSQL` 数据访问
- `JWT` 鉴权与统一响应结构
- `OpenAPI` 导出与 Rust client 生成
- `sidecar` 生命周期管理、版本注入、日志初始化
- 本地打包、部署、升级脚本

## 项目定位

这个模板更适合下面这类项目：

- 需要长期维护的 Rust backend service
- 同时暴露 `HTTP API` 和本机 `IPC` 能力的进程型应用
- 需要比较稳定的目录分层，而不是把业务、协议、配置全部堆在一个 crate 里
- 希望新项目启动时就带上配置、日志、发布、文档和测试约束

如果你只是要写一个一次性小工具，这个模板会偏重；如果你要做一个可部署、可扩展、可维护的服务，它会比较顺手。

## 仓库结构

```text
.
├── src
│   ├── main.rs                 # CLI entry / version / repo-root bootstrap
│   ├── cmd                     # config / ipc / run commands
│   ├── api/http                # HTTP server, OpenAPI doc, generated client
│   ├── core                    # db / service / domain model
│   └── kit                     # config / jwt / error / response / context
├── crates/sidecar             # reusable lifecycle / repo / log / version crate
├── deploy                     # package layout and shell-based deploy scripts
├── tests                      # style and dependency guard tests
├── justfile                   # common dev/build/release commands
└── src/bin/export_openapi.rs  # export openapi.json
```

## 给使用者

### 1. 环境要求

- Rust toolchain：日常 `build/test` 用 stable 即可，`fmt/clippy/fix` 依赖 `nightly`
- PostgreSQL：当前实现运行主进程时需要可用数据库
- `openapi-generator`：仅在需要重新生成 Rust client 时安装
- `cross`：仅在需要构建 `linux-amd64` 包时安装

### 2. 运行时目录约定

应用把 `repo root` 当作运行目录。默认使用当前工作目录，也可以用 `--repo-root` 指定其他目录。运行时会在该目录下读取或生成这些文件：

- `config.toml`：主配置文件
- `.env`：可选环境变量文件
- `logs/`：日志目录
- `process.pid`：进程 PID
- `ipc.sock`：Unix socket 文件

### 3. 生成默认配置

先编译：

```bash
cargo build
```

再生成默认配置：

```bash
./target/debug/rs-project-startup --repo-root . config generate-default
```

也可以先查看当前生效配置：

```bash
./target/debug/rs-project-startup config show
```

默认配置大致如下：

```toml
[db]
enable = false
host = "127.0.0.1"
port = 5432
username = "zunkk"
password = "zunkk"
database = "model"
schema = "public"
ssl_mode = "disable"
log_sql = false

[http]
enable = false
port = 8080

[http.swagger]
enable = true
host = "http://127.0.0.1"

[http.jwt]
token_valid_duration = "3days"
token_hmac_key = "rs-project-startup-hmac-key@2509"

[log]
level = "debug"
max_log_files = 14
```

### 4. 最小可运行配置

当前实现里，`Service::start()` 会在启动时尝试创建 `user` / `user_auth` 表，所以默认的 `db.enable = false` 配置不能直接运行主进程。要成功执行 `run`，你至少需要启用 PostgreSQL。

一个最小可运行示例：

```toml
[db]
enable = true
host = "127.0.0.1"
port = 5432
username = "postgres"
password = "postgres"
database = "model"
schema = "public"
ssl_mode = "disable"
log_sql = false

[http]
enable = true
port = 8080

[http.swagger]
enable = true
host = "http://127.0.0.1"

[http.jwt]
token_valid_duration = "3days"
token_hmac_key = "change-me-in-production"

[log]
level = "info"
max_log_files = 14
```

启动：

```bash
./target/debug/rs-project-startup --repo-root . run
```

### 5. 环境变量覆盖

配置支持 `default < config.toml < environment` 的覆盖顺序。  
环境变量前缀来自应用名 `rs-project-startup`，会转换为 `RS_PROJECT_STARTUP_`。

例如：

```bash
RS_PROJECT_STARTUP_HTTP_ENABLE=true \
RS_PROJECT_STARTUP_HTTP_PORT=8081 \
./target/debug/rs-project-startup config show
```

### 6. HTTP 能力

当 `http.enable = true` 时，应用会监听 `0.0.0.0:<port>`。

当前可见接口包括：

- `GET /ping?content=ping`
- `GET /api/v1/user/login`
- `GET /api/v1/user/refresh-token`
- `POST /api/v1/user/register`，但这个接口当前被限制为仅允许从 `IPC` 入口访问

如果同时开启 `http.swagger.enable = true`，还会挂出：

- `GET /swagger-ui`
- `GET /swagger-ui/openapi.json`

### 7. 用户鉴权流程

当前实现的用户链路是：

1. 先通过 `IPC` 创建用户
2. 再通过 `HTTP login` 获取 JWT
3. 使用 `Bearer <token>` 调用 `refresh-token`

`register` 之所以走 `IPC`，是因为路由上显式要求 `from_ipc`，对外部 HTTP 请求会返回错误。

### 8. IPC 使用方式

应用启动后会在运行目录创建 `ipc.sock`，并通过 Unix socket 暴露内部 API。

当前 CLI 已实现的 IPC 命令是用户注册：

```bash
./target/debug/rs-project-startup --repo-root . ipc user register \
  --role admin \
  --auth-type username \
  --auth-id admin \
  --auth-token admin123 \
  --name admin \
  --desc "bootstrap user"
```

只有在应用正在运行且 `ipc.sock` 存在时，这个命令才会成功。

### 9. 常用命令

```bash
cargo check --workspace
cargo test --workspace
just fmt
just clippy
just opt-code
just build
just release
```

### 10. 打包与部署

本仓库已经带了 shell 部署脚本：

- `just package`：生成本地发布包
- `just package-linux-amd64`：生成 Linux AMD64 发布包
- `deploy/start.sh`
- `deploy/stop.sh`
- `deploy/restart.sh`
- `deploy/status.sh`
- `deploy/update_binary.sh <new_binary>`

部署包内部通过 `deploy/bin_proxy` 调用 `deploy/tools/bin/app`，并把 `deploy/` 根目录当作运行目录。

## 给维护者

### 1. 架构分层

当前代码大致可以理解成下面这条链路：

```text
main.rs
  -> cmd
  -> Repo<Config>
  -> App
     -> Core
        -> DB
        -> Service
           -> user::Service
     -> HTTP Server
        -> HTTP routes / OpenAPI / IPC socket
```

各层职责建议保持如下边界：

- `src/cmd`：只负责 CLI 入口，不承载业务规则
- `src/api/http`：只处理协议层、鉴权前置检查、OpenAPI 暴露
- `src/core/service`：承载业务行为
- `src/core/model`：承载持久化模型和 schema/index 定义
- `src/kit`：放横切能力，如 config、jwt、error、response
- `crates/sidecar`：放可复用的基础设施组件，不要把业务耦合进来

### 2. 关键实现约束

- `register` 当前只允许 IPC 调用，这是显式设计，不是文档遗漏
- `run` 启动时会创建用户相关表，因此数据库是启动前置条件
- `Response<T>` 固定使用 `200 OK + code/msg/data` 的统一响应格式
- JWT HMAC key 默认写在配置里，生产环境必须覆盖
- `Repo` 会先读默认值，再读 `config.toml`，最后读环境变量

### 3. OpenAPI 与生成代码

OpenAPI 文档由 `src/bin/export_openapi.rs` 导出，Rust client 代码由 `just generate-openapi-client` 刷新。

相关路径：

- `src/bin/export_openapi.rs`
- `src/api/http/server.rs`
- `src/api/http/user.rs`
- `src/api/http/client/apis`
- `src/api/http/client/models`

当你修改了 API path、request/response schema 或鉴权要求后，建议同步执行：

```bash
cargo run --bin export_openapi
just generate-openapi-client
```

注意：`src/api/http/client/apis` 与 `src/api/http/client/models` 属于生成代码，不要手工做业务级修改。

### 4. 代码质量约束

仓库里已经有几类测试在兜底：

- `tests/log_message_style.rs`：`tracing` 日志文本必须以大写字母开头
- `tests/error_message_style.rs`：error string 必须以小写字母开头
- `tests/reqwest_tls_feature_case.rs`：`reqwest` 必须显式保留 TLS backend feature

这和仓库约定是一致的：

- log message：`Uppercase` 开头
- error string：`lowercase` 开头

### 5. 日常维护命令

```bash
cargo check --workspace
cargo test --workspace
cargo +nightly fmt --all
cargo +nightly clippy --fix --all --all-features --allow-staged --allow-dirty
cargo +nightly fix --allow-staged --allow-no-vcs --workspace
```

如果你用 `just`，对应 recipe 已经封装在：

- `just check`
- `just test`
- `just fmt`
- `just clippy`
- `just fix`
- `just opt-code`

### 6. 版本与构建信息

`build.rs` 会把这些信息注入到二进制：

- app version
- git branch
- git commit
- build time
- rustc / cargo / sysinfo

应用不带子命令直接运行时，会输出当前版本信息；日志启动时也会打印这些元数据。

### 7. 新项目初始化方式

如果你要把这个仓库当模板改造成自己的项目，可以使用：

```bash
just --set app-name your-app \
     --set app-description "Your Rust service template" \
     init-project-from-template
```

这个 recipe 会做几件事：

- 替换 `Cargo.toml` 中的包名和描述
- 删除模板仓库特有的作者、主页、仓库等元数据
- 把本地 `sidecar` 依赖切换为 Git 依赖
- 更新 `src/bin/export_openapi.rs` 里的 crate 名称

执行后建议再人工检查一次：

- `Cargo.toml`
- `justfile`
- `deploy/`
- OpenAPI 导出二进制里的 crate path

### 8. 适合继续补强的方向

如果你准备把这个模板长期维护下去，优先建议补这些内容：

- 增加 migration 机制，避免只靠运行时建表
- 给 `config`、`run`、`ipc` 增加稳定的 `--help` 回归测试
- 为 `HTTP` / `IPC` / `service` 增加 integration test
- 拆出更明确的 domain/service/repository 边界
- 为 deploy 脚本补上环境校验和健康检查

## License

`Apache-2.0`

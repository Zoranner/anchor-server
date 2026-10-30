# anchor-server

Anchor Guard 服务端仓库，包含 Rust API、浏览器管理台、PostgreSQL schema 和 Docker 部署定义。

## 目录

- `src/`：Axum API、设备认证、策略签发和 SeaORM schema。
- `web/`：Nuxt 管理台。
- `deploy/`：Docker Compose 和部署配置。
- `crates/shared/`：`anchor-shared` 子模块。
- `Dockerfile`：单租户服务端镜像。

## 构建

```text
cargo build --release --locked
```

镜像构建使用仓库根目录作为上下文：

```text
docker build -t anchor-server .
```

## 验证

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# RustVNC 配对后台（rv-server）镜像
#
# 构建上下文必须是仓库根目录：
#   docker build -f deploy/server.Dockerfile -t rustvnc/rv-server .
#
# 说明：
#   * rusqlite 开启 bundled 特性，会在构建阶段编译 SQLite C 代码，
#     因此 builder 必须安装 build-essential（cc/make）。
#   * reqwest 使用 rustls，不依赖 OpenSSL 开发包。
#   * 只复制 workspace 清单与 crates/；apps/pc 只需要清单参与 workspace 解析。
# ---------------------------------------------------------------------------

FROM rust:1.99-bookworm AS builder

WORKDIR /src

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps

RUN cargo build --release --locked -p rv-server

# ---------------------------------------------------------------------------

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 rvnc \
    && useradd --uid 10001 --gid rvnc --home-dir /data --no-create-home --shell /usr/sbin/nologin rvnc \
    && mkdir -p /data \
    && chown rvnc:rvnc /data

COPY --from=builder /src/target/release/rv-server /usr/local/bin/rv-server

ENV RV_BIND=0.0.0.0:8080 \
    RV_DB=/data/rvnc.db

WORKDIR /data
USER 10001:10001
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/rv-server"]

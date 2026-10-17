# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# 自建 DERP 中继（derper）镜像
#
# 构建：
#   docker compose -f deploy/docker-compose.yml build derper
#
# DERP_VERSION 应与客户端 tailscale.com 版本尽量接近。
# 一般不需要 --verify-clients：本容器内没有本地 tailscaled。
# ---------------------------------------------------------------------------

FROM golang:1.24-alpine AS build

ARG DERP_VERSION=v1.80.0

ENV CGO_ENABLED=0
RUN apk add --no-cache git
RUN go install tailscale.com/cmd/derper@${DERP_VERSION}

# ---------------------------------------------------------------------------

FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata \
    && mkdir -p /var/lib/derper /certs

COPY --from=build /go/bin/derper /usr/local/bin/derper

# derper 需要绑定容器内的 443/TCP 与 3478/UDP。
# 容器内由 Docker 放开低端口绑定；如使用 rootless Docker 且绑定失败，
# 把命令中的 -a=:443 改为 -a=:8443 并同步修改端口映射。
EXPOSE 443/tcp 3478/udp

ENTRYPOINT ["/usr/local/bin/derper"]

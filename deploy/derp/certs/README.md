# derper 证书目录

`docker-compose.yml` 会把本目录以只读方式挂载到 derper 容器的 `/certs`。

默认 `DERP_CERTMODE=manual`，derper 按主机名查找以下两个文件：

```text
certs/derp.example.com.crt   # 完整证书链（PEM）
certs/derp.example.com.key   # 私钥（PEM，建议 600）
```

文件名中的 `derp.example.com` 必须与 `.env` 里的 `DERP_DOMAIN` 完全一致。

## 获取证书

推荐用 DNS-01 方式签发（不占用 80/443 端口，适合与 Caddy 同机部署）：

```bash
# 示例：certbot + DNS 插件（按你的 DNS 服务商替换插件）
certbot certonly --dns-cloudflare \
  -d derp.example.com \
  --cert-path ./certs/derp.example.com.crt \
  --key-path ./certs/derp.example.com.key
```

或者从已有 ACME 客户端复制 `fullchain.pem` 与 `privkey.pem` 并按上面的文件名重命名。

## 使用 Let's Encrypt 模式

如果 derper 独占一台公网主机（80/443 都可用），可在 `.env` 中设置：

```dotenv
DERP_CERTMODE=letsencrypt
DERP_HTTPS_PORT=443
```

并把 compose 中 derper 的端口映射改为 `"80:80/tcp"`、`"443:443/tcp"`。
**不要让 Caddy/Nginx/负载均衡代理 derper**：DERP 需要端到端直连才能正确工作。

> 不同 derper 版本的 manual 证书文件名可能略有差异；若启动日志提示找不到证书，
> 请以 `derper --help` 与对应版本源码为准。

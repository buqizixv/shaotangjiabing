# 在途密钥网关

Python 3.11+ 标准库服务，无额外依赖。Ubuntu 24.04 自带 Python 可运行。仅监听 127.0.0.1:8080，由 Caddy 提供公网 HTTPS。上游 MiniMax、高德/可可地图密钥只存于服务器，不下发给客户端。北京公交公开接口仍走原来的直连。

## 本项目 IP / 自签证书部署

本项目演示网关使用 IP 与公开自签证书。独立部署可运行 gateway/deploy-ip.ps1；密钥登录时使用 deploy_ip.py 的 --key 与 --fingerprint 参数，分别传入你自己的私钥路径和通过可信渠道核对的主机指纹。不要提交这些机器专用配置。

部署窗口确认后，可以使用本机已有 MiniMax/地图密钥，也可以隐藏输入新的密钥。程序只上传 backend 和 gateway 源码，使用 sudo 安装服务，生成带 IP SAN 的一年期自签证书，保存上游密钥到服务器 root 只读文件，自动生成网关访问凭证并存入本机 DPAPI。客户端只信任下载的指定证书（gateway-ca.pem），保留 TLS 主机名和证书验证，不使用 verify=false。此公开证书与仅含 URL 的 gateway-client.json 可随客户端分发；证书私钥不可以。

程序仅给 UFW 添加 80/443 规则，不清空防火墙、不调整 SSH，也无法修改云安全组。腾讯云安全组需另外放行 TCP 80/443。已有 Caddy 配置通过 import 加入专用配置；保留其他网站。重新部署复用证书，但会更换本演示凭证，旧客户端须更新。证书到期前需更新服务端证书和客户端信任文件。

使用 IP 的客户端通常不会发送 TLS SNI，Caddy 全局 default_sni 配置为服务器 IP 以选择对应证书。如果已有其他 default_sni，不覆盖它，须另行设置合适的 TLS 连接策略。公网 health 成功后仍须验证真实上游；本次部署已验证地点查询、通勤路线、天气与 AI 总结。

## 部署

1. 将 `backend/` 与 `gateway/` 上传至 `/opt/onway-gateway/`。不要上传 installed、_debug、devkeys 或本机密钥。
2. 创建服务用户：`sudo useradd --system --home /var/lib/onway-gateway --shell /usr/sbin/nologin onway-gateway`。
3. `sudo install -d -m 700 /etc/onway-gateway`，复制 secrets.env.example 为 `/etc/onway-gateway/secrets.env`，在服务器上填写密钥；`sudo chmod 600 /etc/onway-gateway/secrets.env`。支持现有可可地图 Key，也支持高德官方 Web 服务 Key。
4. 运行 `python3 gateway/create-token.py`，凭证输入不回显；将输出的哈希填写进 ONWAY_TOKEN_HASHES。多个凭证哈希用逗号隔开，每个有独立额度。客户端保存原始访问凭证，上游密钥不交给客户端。
5. `sudo cp gateway/onway-gateway.service /etc/systemd/system/`，执行 `sudo systemctl daemon-reload` 与 `sudo systemctl enable --now onway-gateway`。
6. 安装 Caddy（按 https://caddyserver.com/docs/install 的 Ubuntu 官方步骤），把示例域名替换成实际域名后加入 `/etc/caddy/Caddyfile`。已有网站时追加配置，不能直接覆盖。`sudo caddy validate --config /etc/caddy/Caddyfile` 后 `sudo systemctl reload caddy`。域名 DNS 指向服务器，云安全组开放 TCP 80、443，8080 无须公网开放。
7. `curl https://你的域名/health` 应返回 ok=true；这只代表服务活着，上游可用性须从应用的服务配置“测试连接”验证。

## 客户端

在应用目录运行 `python -m backend.v060.configure_gateway`，输入实际 HTTPS 域名与访问凭证。域名写入 gateway-client.json，凭证存入本机 DPAPI；启动时自动通过网关请求。也支持 ONWAY_GATEWAY_URL / ONWAY_GATEWAY_TOKEN 环境变量。只分发 gateway-client.json（只有 URL），不分发上游密钥。不同用户需要各自的网关凭证；不把所有用户共用的无限额度凭证放到公开 GitHub。演示安装包可预配置有额度的访问凭证，但它可被提取，不是秘密，须按演示额度与可撤销凭证管理。

移除 gateway-client.json 且取消 ONWAY_GATEWAY_URL 后恢复原来本机密钥直连。配置网关后请求失败不会自动回落到本机密钥，避免绕过额度或把调用发错位置。

## 边界和限制

- 仅开放应用所需的 6 个地图路径，以及 intent/recommend/assist/summary/memory AI 任务；不接受任意上游 URL、模型或聊天 prompt。
- AI 固定使用项目提示词，最多 1200 输出 token；请求最多 96KB，同时处理最多 8 次上游请求。
- 默认每凭证每天 AI 100 次、地图 2000 次；全站分别 1000/10000 次；每分钟 AI 10 次、地图 120 次。失败的上游尝试也计次。每日按北京时间重置，计数存 SQLite，重启不清零。
- 这些是调用次数上限，不是精确金额预算；需要在上游平台设置余额/消费限制。
- 无公开发放凭证接口，无管理接口，无跨域开放。令牌撤销：移除其哈希并重启服务。
- 服务只保存凭证哈希及计数，不保存位置、AI 上下文或响应；数据仍会按功能发送至对应上游。Caddy 未启用请求访问日志。

## 验证

在应用目录运行 `python -m unittest gateway.test_server backend.v060.test_gateway`。涵盖身份验证、路径白名单、固定上游、重启后额度、请求大小、任务约束及客户端不发送上游密钥。

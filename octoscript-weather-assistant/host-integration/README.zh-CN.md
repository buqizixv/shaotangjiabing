# Daycast 的 OctoSense 宿主桥接

[English](README.md) | 简体中文

仅更新 bundle 不足以获得 Matrix 消息发送能力。`octosense-rinx.patch` 面向本机已集成 Daycast 系统应用的 OctoSense 工作区，包含宿主改动和系统 bundle 界面；它不是面向全新上游 OctoSense 仓库的补丁。

桥接验证宿主提供的 Daycast 应用 ID、前台交互权限、固定的聊天室读取/发送方法及精确参数，通过已有 AI bus 调用正在运行的 Rinx 原生 `list_rooms`/`send_message`。Rinx 负责确认、账号凭据及加密；只有实际原生结果返回后才回答 Daycast。未回答的请求两分钟后取消，不会将超时当作成功。没有登录、令牌输入、原始 HTTP 发送、任意工具调用或代理身份伪造。

UI 请求仍受 App Hub 的 `storage` 能力限制，宿主再次验证身份。修改只扩展已有 Daycast 存储服务；`may_prompt=false` 的代理工具、首页小卡片等调用被拒绝。Rinx 必须已打开并登录，Daycast 的 Rinx 提醒默认关闭。

本机执行过以下命令：

```powershell
python -X utf8 tools/setup.py
cargo test --locked -p octosense-llm-service --test weather_advice
cargo test --locked -p octosense-shell --features app-hub,app-rinx daycast_rinx --lib
cargo build --locked --release -p octosense --bin octosense
```

补丁包含桥接测试。`weather-advice.rs` 是实际执行的 Splash 测试套件副本，对应本机 `apps/weather-assistant/tests/advice.rs` 及 LLM 服务的测试目标。`system-manifest.json` 和 `system-listing.json` 是重新计算摘要后的系统 bundle 元数据。

在其他预集成工作区应用补丁、移动设备构建均**尚未验证**。真实家人收件仍需用户登录后验收，独立 card-host 只可检查界面，没有 Rinx 桥接。关闭 Daycast 后停止天气检查，不提供系统后台调度。

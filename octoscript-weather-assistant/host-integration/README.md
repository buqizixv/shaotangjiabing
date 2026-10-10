# Daycast 宿主接入代码

这里归档本次 UI 已运行版本所使用的宿主接入，避免将整个 OctoSense 仓库放进应用仓库。

- `host-service/`：日程持久化与校验、日程代理工具声明，以及 Rinx 桥接入口；依赖由 OctoSense 根 workspace 提供。
- `octosense-daycast.patch`：相对于 OctoSense `127ae4b` 的服务注册、Cargo 依赖、系统应用目录、日程代理审批、Rinx 请求路由和 19 项应用回归测试。
- `../tests/advice.rs`：页面解析、真实预报边界、日程表单与跨天事件、AI 请求及家庭提醒回归测试。

应用 bundle 保持商店应用 ID `weather-assistant`。此前验证使用系统应用 ID `os.weather-assistant`；同一宿主服务接收这两个 ID。商店安装的真实模型、已登录 Rinx 发送，以及最新 OctoSense 主分支仍未验证。补丁基于固定旧版本，不能直接声称适配最新主分支。

## 原验证环境的集成方式

下面是接入步骤说明，尚未在本仓库迁移后的新副本中执行：

1. 在 OctoSense `127ae4b` 的独立工作树执行 `git apply --check` 检查此补丁，再应用补丁；已有改动时应人工核对冲突。
2. 把本项目 `bundle/` 复制到 OctoSense 的 `apps/weather-assistant/bundle/`；作为内置系统应用时，将复制后的 manifest ID 改为 `os.weather-assistant`。
3. 用配套 App Hub 的 `hub stamp` 重新计算复制后的 bundle 摘要。不能沿用不同文件集合的摘要。
4. 按 OctoSense 自身的 AGENTS.md 准备固定版本依赖，再运行测试和构建。

## 已执行的验证

下列命令在原 OctoSense 验证分支执行通过，本仓库没有独立 Cargo workspace：

```powershell
python -X utf8 tools/setup.py --check --cargo
cargo test --locked --offline --release -p octosense-llm-service --test weather_advice
cargo check --locked --offline --release -p octosense
cargo check --locked --offline --release -p octosense --features mobile-apps
```

19 项回归测试通过；Windows 桌面默认与 mobile-apps 特性编译通过。Android/iOS 设备未验证，Android 全量依赖图检查未完成。相机源在所验证 Windows 运行时中不支持，不能拍摄照片。

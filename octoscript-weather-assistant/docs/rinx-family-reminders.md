# Rinx 家庭天气提醒

此功能需要本次升级的 OctoSense 宿主：宿主将 `storage.daycast.rinx.rooms` 和 `storage.daycast.rinx.send` 接到内置 Rinx。普通 App Hub 的独立 card-host 没有这个桥接，应用会明确显示不可用。宿主修改见 `host-integration/`，不能只更新 bundle 就获得消息发送能力。

1. 按 [Rinx 注册与使用说明](https://github.com/gosimfoundation/hackathon-agenticapp26/blob/main/docs/rinx-guide.md) 注册账号。在 OctoSense 内打开 Rinx，服务器填写 `https://matrix.rinx.chat`，通过浏览器在 `https://auth.matrix.rinx.chat` 登录。
2. 在 Rinx 创建或加入家人聊天室，用家人的完整账号（例如 `@dad:matrix.rinx.chat`）邀请他们加入。Daycast 使用已加入的聊天室 ID，不直接向账号 ID 发送消息。
3. 在 Daycast → 我的添加家庭城市，点击「读取 Rinx 聊天室」，选择家人聊天室。所有家庭城市的提醒发送到这个聊天室；更换聊天室会关闭 Rinx 提醒，需重新开启。
4. 开启「家庭天气变化检查」和「变化后 Rinx 提醒」。首次检查只建立基线，之后在应用打开期间每 15 分钟检查各家庭城市，也可点击「立即检查家庭天气」。
5. 检测到变化后，Daycast 显示天气正文并请求 Rinx 发送。到 Rinx 确认目标聊天室和完整正文；只有原生客户端报告成功才显示「已通过 Rinx 发送」。也可以在提醒卡片上手动发送。

规则为开始降水、较上次检查升降温至少 5°C、进入 ≥35°C 高温或 ≤0°C 冰点、风速从低于 40 km/h 上升到该阈值。提醒使用 Open-Meteo 当前天气，标注城市当地观测时间，不由 AI 编造。首次检查、重复/旧时间戳、无关的小变化不触发；每个城市独立比较。

Rinx 提醒默认关闭。开关只作用于之后的新变化，历史草稿需手动发送；重启不自动重试旧草稿。最近 20 条提醒和天气基线保存在应用私有目录。关闭 Daycast 后停止检查；不提供操作系统后台或全天候推送。

网络失败、未登录、未加入聊天室、Rinx 未打开或用户拒绝时，不会记为已发送。失败后可以手动重试。若应用在发送中关闭，下次启动显示「结果未知」，请先检查 Rinx 中是否已有该消息，再决定重试。加密和账号凭据均由 Rinx 的 Matrix SDK 会话处理，Daycast 没有密码、访问令牌或恢复密钥输入框。

验证：本次本地测试覆盖真实 Splash 函数、消息状态和宿主桥接路由；构建及界面验证结果见交付说明。真实 Rinx 账号及家人收件确认需人工验收，本次未向真实家人发送测试消息。

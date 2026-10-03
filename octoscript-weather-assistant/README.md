# OctoScript 天气助手

这是面向 OctoSense App Hub 的 contained OctoScript 天气应用，正式包位于 `bundle/`。

## 功能

- 首页显示当前城市天气摘要、穿衣建议入口和家庭关注摘要。点城市名进入城市管理。
- 城市管理可查看和切换已保存城市；搜索页提供国内外热门城市，也支持输入城市名并选择匹配地点。
- 天空页展示逐小时与未来七天预报。
- 我的页面保存体感偏好、自定义天气标签、家庭关注城市和本机提醒偏好。
- 天气问答使用 OctoSense 宿主的 `octos.session.open` / `octos.turn.start`，依赖用户配置的 AI 并在首次使用时授权。

天气数据与城市搜索使用 Open-Meteo。家庭城市和提醒目前只保存在本机，不会发送 Matrix 消息或系统通知。

## 开发与验证

在 OctoScript-App-Design-Flow 工具链和 OctoSense App Hub 的 card-host 上运行 `tools/octo check bundle`、`tools/octo run bundle`。当前本机 card-host 没有 OctoSense 助手服务，因此它只能验证问答的服务不可用提示；真实 AI 回复需在支持 `octos` 的 OctoSense 宿主中完成授权后验证。

截图来自本机 card-host 的实际运行界面，保存在 `bundle/screenshots/`。天气 API 在此运行环境的联网状态不稳定；如果首页显示网络错误，请检查宿主网络后刷新。

## 发布状态

这是仓库中的应用源码和开发包。App Hub 发布还需要发布者签名密钥、最终发布者资料、平台确认和人工提交。

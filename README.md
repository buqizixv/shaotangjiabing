# Daycast

Daycast 是运行在 OctoSense 上的天气与生活安排助手，版本 **v0.4.4**。它把实时天气、衣橱、天气偏好、日程和家庭关注城市放在同一条生活决策链中，帮助用户把天气预报转成可执行的出门与安排建议。

## 本次版本

- 统一重做今天、天空、问答、城市管理、我的、衣橱、日程和家庭提醒页面
- 支持宽窄窗口，使用原生文字与 SVG 图标
- 完善逐小时预报、未来七天趋势、日出日落、跨天日程和日期编辑
- 支持家庭关注城市与天气提醒偏好
- 修复家庭天气卡片中的日期显示问题
- Daycast 应用清单版本更新为 `0.4.4`

## 项目入口

- 应用源码：[`octoscript-weather-assistant/bundle/main.splash`](octoscript-weather-assistant/bundle/main.splash)
- 应用清单：[`octoscript-weather-assistant/bundle/manifest.json`](octoscript-weather-assistant/bundle/manifest.json)
- 宿主接入说明：[`octoscript-weather-assistant/host-integration`](octoscript-weather-assistant/host-integration)
- UI 改版记录与验证截图：[`octoscript-weather-assistant/docs/ui-redesign-2026-10-10`](octoscript-weather-assistant/docs/ui-redesign-2026-10-10)
- 隐私说明：[`octoscript-weather-assistant/PRIVACY.md`](octoscript-weather-assistant/PRIVACY.md)

## 运行

Daycast 使用 OctoScript 编写。准备好 OctoScript-App-Design-Flow 与 OctoSense App Hub 工具后，可以运行：

```powershell
python <OctoScript-App-Design-Flow 路径>\tools\octo check octoscript-weather-assistant\bundle
python <OctoScript-App-Design-Flow 路径>\tools\octo run octoscript-weather-assistant\bundle --port 8141
python <OctoScript-App-Design-Flow 路径>\tools\octo shot 8141 preview.png
```

完整问答、日程和家庭提醒能力需要支持相应 OctoSense 宿主能力，并由用户完成授权与模型设置。天气和地点搜索需要网络连接。

## 数据与隐私

天气和地理编码使用 Open-Meteo 服务。城市、衣橱、偏好、日程、家庭城市与提醒设置保存在 Daycast 的应用私有空间。衣物照片保存在本机，不发送给天气服务或 AI。AI 生成的资料变更会先展示草案，用户确认后才保存。

## 版本

当前版本：**daycast-v0.4.4**

- Git 标签：[`daycast-v0.4.4`](https://github.com/buqizixv/shaotangjiabing/releases/tag/daycast-v0.4.4)
- 最新提交：[`5d95270`](https://github.com/buqizixv/shaotangjiabing/commit/5d9527086d14b2c479aebd7866a76e28905a819a)
- 许可证：Apache-2.0

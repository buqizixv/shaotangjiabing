# Daycast

Daycast 是运行在 OctoSense 上的天气与生活安排助手，源码版本 **v0.6.0**。它把实时天气、衣橱、天气偏好、日程和家庭关注城市放在同一条生活决策链中，帮助用户把天气预报转成可执行的出门与安排建议。

## 本次版本

- 出行 Plan 结合天气、衣橱、偏好和已有日程判断准备度；只追问信息缺口，含糊周末时间会先让用户选择
- 出行计划包含逐日安排、衣橱穿搭、打包清单、天气备选和流程图；确认后同步到 Daycast 与 OctoSense 系统日历
- 仅在应用打开时检查天气变化，调整前须用户确认；不监测航班变化
- 固定的暂停入口保留需求与草稿，由用户主动继续；重启不会自动恢复模型请求或日程写入
- 完整计划包含穿搭、明确标注的默认安排，以及带天气备选分支的原生流程图
- 支持从已保存日程打开完整计划与流程图，并在窄窗口收起侧栏文字
- 统一重做今天、天空、问答、城市管理、我的、衣橱、日程和家庭提醒页面
- 支持宽窄窗口，使用原生文字与 SVG 图标
- 完善逐小时预报、未来七天趋势、日出日落、跨天日程和日期编辑
- 支持家庭关注城市与天气提醒偏好
- 修复家庭天气卡片中的日期显示问题
- 修复普通聊天的日程草案显示与连续对话；冲突草案保留供修改
- Daycast 应用清单版本更新为 `0.6.0`；本次未创建新的 GitHub Release

## 项目入口

- 应用源码：[`octoscript-weather-assistant/bundle/main.splash`](octoscript-weather-assistant/bundle/main.splash)
- 应用清单：[`octoscript-weather-assistant/bundle/manifest.json`](octoscript-weather-assistant/bundle/manifest.json)
- 宿主接入说明：[`octoscript-weather-assistant/host-integration`](octoscript-weather-assistant/host-integration)
- UI 改版记录与验证截图：[`octoscript-weather-assistant/docs/ui-redesign-2026-10-10`](octoscript-weather-assistant/docs/ui-redesign-2026-10-10)
- 连续对话与草案测试：[`octoscript-weather-assistant/docs/dialogue-fix-2026-10-10`](octoscript-weather-assistant/docs/dialogue-fix-2026-10-10)
- 出行 Plan 功能说明与实际验证：[`octoscript-weather-assistant/docs/travel-plan-confidence-2026-10-11`](octoscript-weather-assistant/docs/travel-plan-confidence-2026-10-11)
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

当前源码版本：**v0.6.0**；此前发布版本：**daycast-v0.4.4**。

- Git 标签：[`daycast-v0.4.4`](https://github.com/buqizixv/shaotangjiabing/releases/tag/daycast-v0.4.4)
- UI 日期修复提交：[`5d95270`](https://github.com/buqizixv/shaotangjiabing/commit/5d9527086d14b2c479aebd7866a76e28905a819a)
- 许可证：Apache-2.0

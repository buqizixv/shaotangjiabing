# 天气助手（OctoScript 应用）

## 目标
把现有 Rust 天气助手的主要使用体验迁移成符合 OctoSense App Hub 的 contained OctoScript bundle。重点按用户给的手机天气截图重新设计城市切换流程；界面采用清晰的中文天气卡片，不复用之前 Rust 桌面窗口的布局。

## 页面与操作
- 今天：当前城市入口（点击进入城市管理）、当前天气/体感/高低温、穿衣与出行建议、家庭城市提醒摘要、常用场景标签。
- 天空：小时预报和未来 7 天预报，展示气温、降水概率与天气状况。
- 问答：聊天式天气问答，通过 `octos.session.open` 和 `octos.turn.start` 使用设备 AI；首次授权，提供服务不可用、未配置 AI、被拒绝等可见状态，并且无 AI 时其余页面仍可用。
- 我的：偏好标签（怕冷、怕热、紫外线、降雨等）和 `+` 自定义标签入口；家庭成员/关注城市列表及本地提醒偏好。
- 城市管理：保存城市卡片，点击切换当前城市；顶部搜索入口进入热门城市页；热门国内/国际城市按钮；搜索输入后展示匹配结果（城市-省州-国家），选择结果后保存并切换。支持空结果与网络错误提示。

## 数据与权限
- `storage`：持久化当前城市、已保存城市、自定义标签和家庭提醒偏好，仅写入应用私有目录。
- `net`：调用 Open-Meteo 天气与地理编码 API；hosts 仅 `api.open-meteo.com` 和 `geocoding-api.open-meteo.com`。
- `octos.session.open`、`octos.turn.start`：通过 OctoSense 自己的助手会话进行 AI 问答，设备端授权与 AI provider 设置由宿主处理，bundle 不包含凭据。
- 不伪称发送 Matrix 消息； contained app 目前没有 Matrix host service。家庭提醒可在应用内配置并在天气页展示。

## 状态与验收
- 首次启动有可用默认城市或清楚的加载提示。
- 天气/地理编码网络失败时保留上次数据并显示错误状态；搜索空结果有说明。
- 城市、偏好和提醒设置重启后仍保存。
- 在 card-host 测完整 UI/城市流程；AI 服务在 card-host 中预期不可用，需要在支持 `octos` 的 OctoSense shell 验证授权和真实回复。

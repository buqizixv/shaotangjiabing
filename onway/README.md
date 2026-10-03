# 在途 Onway · 原型对齐版 0.4.2

此版本把 `prototypes/01-*.html` 至 `17-*.html` 转换为可点击的原生 OctoSense 卡片。欢迎页、设置、方案、通勤阶段、异常和历史均已核验。背景、文字、滚动区和按钮是原生组件，图标与渐变是矢量 SVG，不使用整页截图充当界面。

本项目独立保存在仓库的 `onway/` 文件夹，与队友的 `octoscript-weather-assistant/` 分开维护。

## 下载与运行

- `bundle/`：当前 0.4.2 应用源码、清单、字体、图标与截图。
- `prototypes/index.html`：可直接在浏览器中打开的 HTML 原型导航，包含 17 个场景。
- `docs/在途_Onway_产品需求文档_V0.1.0.md`：产品需求文档。
- `controller.splash`、`sync-prototypes.cjs`：业务控制器与原型转换脚本。
- `check-prototype-pages.cjs`：原生宿主页面与交互检查脚本。

运行原生应用需要另行准备 [OctoScript-App-Design-Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) 和 [OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub) 工具链。以下命令在 `onway/` 内执行，替换工具链路径：

```text
python <OctoScript-App-Design-Flow路径>/tools/octo run bundle --port 8141
```

核验仓库内的现有签名包时，指定开发公钥（公钥可公开，不包含私钥）：

```text
python <OctoScript-App-Design-Flow路径>/tools/octo check bundle --publisher-key dev=ea6e78d1aab7224923f9e014e9d6d87d6187a4f19839d1e312affd2cbda0074e
```

如需重新生成界面，先准备 Node.js 18 或更新版本及 Playwright：

```text
npm install --no-save --package-lock=false playwright
npx playwright install chromium
node sync-prototypes.cjs
python <OctoScript-App-Design-Flow路径>/tools/octo check bundle
```

当前清单含本地开发签名；重新生成会使旧签名失效，需要使用自己的开发签名流程重新打戳、签名和检查。发布者与隐私政策仍是占位信息，正式发布前需替换。

此目录不包含宿主工具链、发布密钥、个人应用数据或调试缓存。真实定位依赖下文描述的新版 Windows 宿主接口；普通宿主不一定支持，HTML 原型可以独立查看。

## 当前范围

- 地点名称、方案选中、偏好和通知阈值能够保存。
- 预览通勤流程可连续操作；预览到达不会写入真实历史。
- 手动补记支持输入耗时；清空历史需要确认，并显示真实空状态。
- Windows 定位桥接使用系统 Geolocator，不需要高德 Key、不以 IP 或模拟坐标代替设备位置。页面导航底部有“检查真实定位”，地点编辑页有“使用当前位置”。检查页区分等待、权限关闭、无数据、不可用、服务异常及精度不足；收到新位置时自动刷新状态，不重建正在编辑的输入框。
- 仅接收最近 60 秒的真实位置；标记地点还要求精度不超过 50 米。系统没有返回位置或精度不达标时，不覆盖已有地点坐标。设备坐标标记为 WGS84，未来接高德路线时须转换为匹配的坐标系，不直接混用。
- 初始路线、班次、位置、图表与对比数据是原型示例，不是实时服务。权限开关目前只保存偏好，不代表已取得系统授权。后台自动记录、实时路线、班次和通知仍需后续接入与设备验证。
- 旧版无 schema 的配置保留为应用数据中的 `legacy-cfg-v0.3.json`；已有历史保留。新界面以原型地点初始化。

## 维护

`controller.splash` 保存基础业务控制器；`sync-prototypes.cjs` 读取原型、测量布局、生成 `bundle/main.splash` 与 SVG。原型文件不被修改。转换时调整了原型中会裁切按钮的弹性布局，超过卡片高度的内容可滚动，底部按钮独立固定。

运行 `node sync-prototypes.cjs` 后，如增加文字，请重新对两个随包字体做字形子集。字体使用 SIL OFL 1.1，许可证随包附带；许可证网址以不带协议的可读形式列出，避免本地资源检查将其误判为网络资源。

按 `AGENTS.md` 的流程运行独立测试实例，再执行 `node check-prototype-pages.cjs`。它核验全部 17 页和首次设置、连续通勤、地点编辑、阈值、历史清空与补记。不要对个人已安装的数据运行完整交互测试。

定位接入需要新版 OctoSense Windows 宿主：`sys.gps("status")` 返回 0 不可用、1 等待、2 有位置、3 未授权、4 无数据、5 异常；`sys.gps("timestamp")` 返回源位置的 Unix 秒时间。原有 `ok/lat/lon/acc` 保持兼容。权限变更后应重新打开 OctoSense；系统设置或设备没有提供真实位置时，应用不能保证取得坐标。当前仍保持预览模式，不启用自动行程或实时到站功能。

若仅检查已安装界面的页面，可设置 `ONWAY_PORT`、`ONWAY_PAGES_ONLY=1` 和独立 `ONWAY_CAPTURE_DIR` 后运行同一脚本；此模式不执行设置编辑、清空或补记，最后返回欢迎页。

## 本次验证

2026-10-03：Windows 新宿主发布构建、现有定位权限测试、运行时补丁锁检查通过。0.4.2 签名包检查通过，加入本地目录第 9 版；没有提交外部商店，发布者与隐私政策仍为本地开发占位信息。

独立 OctoSense 原生实例重新核验全部 17 页、首次设置、连续通勤、地点编辑、阈值、历史清空及补记。真实 Windows 定位返回位置，测试时精度约 76 米，未达到产品 50 米标记门槛；“使用当前位置”正确拒绝覆盖已保存坐标，并保留未保存输入，定时状态刷新也未重置输入。没有使用模拟定位，也没有把测试记录写入个人数据。尚未实测精度达标后的坐标保存及其他平台。

上述历史验证记录来自开发工作区；调试目录和定位日志未上传。仓库内保留的应用截图位于 `bundle/screenshots/`。

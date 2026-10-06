# App Hub 首次审核提交 · 1.0.2

本次采用 unsigned 首次提交。官方规则允许首发不签名；后续一旦登记发布者签名密钥，更新必须遵守密钥连续性。发布者标识为 `aimfx06`，公开联系身份为 GitHub 用户 AimFx06。

提交包路径：`onway/bundle`。它只包含界面脚本、manifest、listing、字体、图标和真实截图。不包含 Python、可执行文件、上游密钥、网关访问凭证或个人运行数据。

## 接入状态

请求人工审核，并请维护者确认原生扩展的交付路径，暂不请求把当前包作为普通宿主完整可用应用直接入库。

当前完整功能依赖：

- `backend/v060/worker.py` 与 `engine.py`：私有数据、命令处理、路线与 AI 任务。
- `runtime/octosense-onway.patch`：配套后台生命周期、定位传递、独立行程窗口及主窗口切换。
- `runtime/makepad-windows-location.patch`：当前 Windows 定位实现。
- `card-v060.splash`：行程卡界面，在当前方案中由配套宿主加载。

普通宿主不会从商店包启动任意 Python，也没有本项目的窗口管理扩展。UI 在首次启动未收到后台快照时，12 秒后显示“此宿主尚未接入在途后台”，可重新检查。该说明修复了无限“正在连接”的展示，不代表迁移已经完成。

官方 PUBLISHING 的 What an app is 说明：需要新增原生运行时代码的应用，应集成到 shell release。下一步需要官方确认接受宿主集成还是要求完全迁移到标准脚本与服务 API；没有伪造服务名、放宽沙箱或把可执行文件塞进 bundle。

## 截图与验证

截图来自本次源码的原生运行，使用隔离数据目录；分别说明普通参考宿主的依赖页面及启动配套后台后的首页。配套后台首页不代表普通 App Hub 安装成功，也不代表完整户外实测。

完整检查输出、扫描问题和作者回答随源码放在本目录的 `apphub-review/` 下。`human-review` 是作者自检建议，不是官方审核结论。

正式提交入口：https://github.com/OctoSense-org/OctoSense-App-Hub/issues 。只开 Submit onway 1.0.2 Issue，不自行编辑官方 catalog、index 或 artifacts。

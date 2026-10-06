# 在途0.6本机后台

0.6.34：预览入口移动到主页常用地点下方，直接读取已保存的家和公司，不要求先保存固定通勤。主页显示请求进度及错误；AI 返回格式异常时仍展示经过查询的路线与天气预览，明确注明 AI 建议暂不可用，且不标记为 AI 推荐。常用地点点开先展示已保存的位置，可点击修改位置；固定通勤只列对应的家和公司。自定义日期移除了 1–7 的说明文字。

0.6.32：出行习惯新增固定通勤，选择已保存的家和公司、上班到达时间、下班时间。默认使用国务院公布的 2026 年节假日和调休工作日；尚未支持的年份暂停官方日期提醒，避免猜测。自定义日期选择周一至周日。

AI 与提醒、定位同时开启，设备最近的有效位置在通勤起点附近，并进入提醒时间窗口后，异步查询真实路线与出发地天气。上班建议出发时间由到达时间减去路线预估与 5 分钟余量计算；雨雪增加至 10 分钟。这不是实时道路拥堵判断。邀请确认仅打开路线选择，不自动开始行程；稍后提醒、当天不去都不修改通勤设置。预览不受通勤日期限制，不创建行程或历史，可在主页常用地点下方点击“预览推荐卡片”。

天气使用高德 `v3/weather/weatherInfo`，沿用现有高德凭据和受保护网关，在服务配置独立显示“天气 API 接口”。地区必须匹配，更新时间超过 4 小时的数据拒绝使用。天气失败不阻断路线查询，并明确显示未获取天气。

AI 记忆上方是个性化偏好，下方是历史记忆。新提示词要求中文概括、明确设置与观察分开、避免重复秒数和逐条罗列，对少量样本与人工纠正保持保守。旧记忆按标题拆分显示，保留原文；下一次后台分析会生成新的简洁格式。

验证：`python -X utf8 -m unittest backend.v060.test_v060 backend.v060.test_bus backend.v060.test_commute`；真实接口与原生界面验证 `python -X utf8 _debug/visual-strict/commute-native-test.py`，仅使用隔离数据。

Python3.11及以上，标准库，无需pip依赖。通过项目启动脚本打开OctoSense，由宿主自动管理 `backend.v060.worker`；关闭主体不会终止后台，关闭宿主才终止。

不要同时运行旧 `backend/server.py` 或旧 `start-ai.ps1`。旧模块仅保留兼容测试，0.6入口位于 `backend/v060/`。

凭据使用当前Windows用户DPAPI加密，受保护配置页只监听本机随机端口并验证访问令牌与来源。应用通过私有命令队列提交操作；worker是唯一状态写入者。完整凭据不进入bundle或日志。

MiniMax固定国内开放平台端点、MiniMax-M3，已验证一次真实连接。地图按凭据类型接入：sk_coco_使用用户指定的map.culture09.xyz、Bearer认证及ok/data封装，其他Key使用高德官方Web服务。网关搜索、坐标转换、地址与步行/公交路线已真实验证。网关认证、额度和频率错误使用固定中文映射；高德错误码只保留五位数字，不把服务原文、完整URL或凭据写入日志。

测试：`python -X utf8 -m unittest backend.v060.test_v060 -v`。独立开发运行（仅测试目录）：`python -X utf8 -m backend.v060.worker --data-root _debug/v060-test-data`；用户日常使用不需要手动后台进程。


## 北京公交实时到站

本机后台通过 `backend/v060/bus.py` 使用 `https://ts-api.tundrey.com` 的车来了封装 API，无需密钥。可在启动前设置 `BUS_API_BASE_URL` 为另一个 HTTPS 实例。网络请求由现有 Python 后台发起，Splash 页面只读写本机私有状态和命令文件。

服务配置新增“北京公交 API 接口”健康检查。当前行程会按公交线路、上车站、下车站顺序匹配北京城市 `027` 的线路方向和物理站台，直接使用线路详情提供的 WGS-84 坐标。准备出发、前往车站、候车及换乘阶段共享查询状态；地铁与纯步行不查询公交预测。未能唯一匹配的站台显示暂无预测。

卡片可见且 OctoSense 在前台时约每30秒查询一次；后台保留刷新命令并复用同一请求。只显示 `realData=true` 且 ETA 有效的预测，按秒数排序选择最近车辆。当前卡片展示车辆线路、方向、预计等待时长和车辆距离；接口的到站时刻、最近成功查询时间保留在后台，用于预测解析和数据过期判断。失败退避并遵守限流等待，90秒后标记过期，不把预测归零作为车辆实际到站或自动上车的证据。方向和行程变化后丢弃旧响应。

验证：`python -X utf8 -m unittest backend.v060.test_v060 backend.v060.test_bus`。本机原生界面验证：`python -X utf8 _debug/visual-strict/bus-live-test.py --cards`，仅使用隔离测试目录，不改用户行程。已在2026-10-06实测良乡西门993路往西红门西站与往窦店公交场站两个方向。其他线路通过相同匹配流程查询，是否有预测取决于上游覆盖。


### AI preferences and memory (0.6.30)
The habits page enables MiniMax and saves mode/route preferences. Selected options have a blue background. The route badge is shown only for a validated MiniMax recommendation; loading and failure are visible. Recommendation, summary, memory and assist use independent per-task limits, so recommendations do not delay the arrival summary. Empty summary responses are treated as failures and retried; a pending summary can resume after worker restart.
AI memory is derived from up to 30 completed records and explicit preferences. Manual corrections and unusually short durations are identified as uncertain evidence. The memory result is persisted after the summary card is closed and included in route recommendation context. Clearing history also clears memory and rejects stale in-flight memory results. Direct cancellation creates neither a completed record nor a memory update.
The shell closes the Onway main UI when a new trip card opens, keeping the shell-owned worker alive. Cancel and closing the arrival summary withdraw the card and open the main app at home.

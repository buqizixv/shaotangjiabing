# 在途 Onway v1.0.1

从准备出发到到达，用动态卡片呈现当下需要的出行信息。这是运行于 Windows OctoSense 的独立项目源码，不包含框架源码、编译产物或个人运行数据。

## 当前功能

- 实际路线查询、常用地点、时间/步行/换乘比较；有效 AI 推荐显示推荐标记，方案由用户选择。
- 准备出发、步行、候车、乘车、到达动态卡片；候车展示预计等待与车辆距离，数据不可用时明确说明。
- 按当前阶段人工纠正。进入行程隐藏主应用，关闭总结或中途结束返回主页。中途结束不创建历史。
- 固定通勤支持上下班时间、官方工作日或自选星期；结合真实路线与天气提供出发邀请，不会自动开始行程，也不判断实时道路拥堵。
- 到达后 AI 总结、个性化偏好与历史记忆；历史支持单条删除和全部清空，清空同步清除 AI 记忆。
- MiniMax、地图和天气通过密钥网关；北京公交公开接口单独查询。服务配置显示连接状态。

## 目录与运行

`bundle/` 是经过检查的 unsigned 开发包；`ui-v060.splash`、`card-v060.splash` 是 UI 源码；`backend/v060/` 是当前后台；`gateway/` 是服务器；`runtime/` 是必要的宿主补丁。v060 是内部模块和存储名称，1.0.0 不改数据格式。

需要 Windows、Python 3.11+、OctoSense 和 OctoScript-App-Design-Flow。后台使用标准库；SSH 部署工具另需 Paramiko。完整桌面体验需要 [runtime/README.md](runtime/README.md) 的补丁，普通未修改宿主不能自动管理本项目后台。

在此目录执行 `python -X utf8 build-v060.py`，再使用安装的 `tools/octo check bundle` 打戳检查；Windows 下设置 OCTO_HUB 为 hub.exe 的绝对路径。按工具本地签名、安装流程将 bundle 加入自己的宿主目录。签名密钥和目录信任锚由安装者管理，不能提交。listing 的 publisher 仍是开发占位信息，本项目没有提交应用商店。

`launch.ps1` 接受 HostExe、DataDir 和 Anchor 三个必填参数。DataDir 必须包含已安装的 `onway/bundle/manifest.json` 和可信目录，不要与正在运行的实例共用数据。它设置后台需要的 ONWAY_APP_DIR、ONWAY_PYTHON 并启动宿主。源码构建、包检查和后台测试已验证；其他电脑从零安装仍未验证。此交付为源码，不是免安装程序。

测试命令：

```powershell
python -X utf8 -m unittest gateway.test_server backend.v060.test_gateway backend.v060.test_initial_history backend.v060.test_v060 backend.v060.test_bus backend.v060.test_commute
```

## 服务与密钥

`gateway-client.json` 只有地址，`gateway-ca.pem` 是公开自签证书信任文件，可以分发。首次使用者仍需自己的网关访问凭证，运行 `python -m backend.v060.configure_gateway` 配置。凭证存入当前 Windows 用户的 `%LOCALAPPDATA%/Onway/credentials.dpapi`，不会随源码分发。

MiniMax 和地图密钥只放服务器；独立部署见 [gateway/README.md](gateway/README.md)。不要提交 secrets.env、私钥、访问令牌或 DPAPI 文件。网关失败不会自动回落绕过额度。

公交到站来自第三方车来了封装接口，可用性和覆盖取决于上游，不等同于官方保障服务；天气使用现有高德适配接口。

## 数据与边界

首次运行默认导入用户指定的 10 条历史及 AI 记忆，见 [backend/v060/data/README.md](backend/v060/data/README.md)。路线来自实际查询，日期和完成用时是编写导入，没有完整户外定位核验。升级、已有数据或清空后重启不会再次导入。

个人配置、行程与历史保存在宿主数据目录的 onway/ 下，不提交。Windows 桌面、实际路线/天气/公交接口、AI 推荐与总结、窗口切换已验证；完整户外连续行程、手机端、休眠后持续定位和系统通知尚未验证或接入。关闭整个 OctoSense 会停止后台。

## v1.0.1 即用演示包

Windows 演示包内置宿主、Python 和独立共享网关凭证，解压后打开 Onway.exe，不需填写凭证。源码不包含该共享凭证；修改网关地址不会把包内凭证发给其他服务器。演示网关按用户授权取消调用次数额度，保留请求大小和并发约束。演示包使用定制宿主，不等于标准 App Hub 安装即可获得全部功能。

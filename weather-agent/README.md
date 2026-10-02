# 天气助手（weather-agent）

这是 OctoSense Desktop 的天气助手应用源码，包含天气首页、天空背景、问答、日程、个人偏好与跨城牵挂功能。

## 项目结构

- `src/`：应用与 OctoSense 托管模块源码
- `src/assets/`：天气背景图片和视频
- `tests/`：应用探针
- `Cargo.toml`：crate 清单

## 构建说明

该应用依赖 OctoSense Desktop workspace 提供的 Makepad 与 OctoSense crates，`Cargo.toml` 使用 `workspace = true` 继承依赖。此目录是应用源码快照，不能单独从这个仓库构建或运行；需要放入并注册到 OctoSense Desktop workspace 后再构建。

本次上传只包含天气助手，不包含 OctoSense Desktop 主程序及其余 workspace 改动。

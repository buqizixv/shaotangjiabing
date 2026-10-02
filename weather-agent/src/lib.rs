// octosense-weather-agent：底部四页（今天 / 天空 / 问答 / 我的）的天气决策助手。
//
// 作为 OctoSense 应用，它有两副面孔：
//
//   * 独立窗格（src/main.rs）——用于开发预览的自己的窗口，没有 OctoSense
//     壳注入的系统助手。
//   * 托管模块（src/module.rs）——宿主进程 `WeatherAgentModule` 直接坐在磁贴里，
//     控制台通过 OctoSense 的按实例隔离助手服务对话。
//
// 两副面孔共用同一套部件和同一份数据：`ai.rs` 把屏幕上的预报和日程做成两个
// 只读工具，挂在 AI 总线上，壳的助手（系统配置的模型）拿它回答天气问题。
//
// 库这一层只负责把脚本模块注册出来：
//
// ```ignore
// fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
//     makepad_widgets::script_mod(vm);
//     octosense_weather_agent::script_mod(vm);   // weather + console + schedule + nav + shell
//     ...
// }
// ```

pub use makepad_widgets;
/// 模块入口：宿主把 `WeatherAgentModule` 直接坐在磁贴里（src/module.rs）。
pub use module::WEATHER_AGENT_MODULE;

pub mod ai;
pub mod agent;
pub mod console;
pub mod care_bridge;
pub mod gallery;
pub mod mine;
pub mod model;
pub mod module;
pub mod nav;
pub mod place_picker;
pub mod schedule;
pub mod shell;
pub mod today;
pub mod standalone;
pub mod ui;
pub mod weather;

use makepad_widgets::{ScriptVm, ScriptValue};

/// 脚本模块注册：今天、天空画廊、问答、我的 + 地区切换 + 导航 + 外壳。
/// `shell` 必须最后：它的默认值里嵌了所有页面类型，那几个类型得先注册好。
pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    today::script_mod(vm);
    gallery::script_mod(vm);
    mine::script_mod(vm);
    place_picker::script_mod(vm);
    console::script_mod(vm);
    nav::script_mod(vm);
    shell::script_mod(vm)
}

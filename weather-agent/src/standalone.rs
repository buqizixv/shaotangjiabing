// 兜底的 broker 自举：shell 那边 broker 注入偶尔掉链子（policy / 启动时序
// 各路原因），让 weather-agent 在 `attach_assistant(None)` 时自己起内核 + 注
// broker，不让用户对着空气说话。
//
// 只有**独立窗格**（`octosense-weather-agent.exe` 自己跑）才需要这条 fallback：
// 那台机器上没有 shell，自己起内核是唯一路径。
//
// 从 shell 磁贴打开的天气助手**不走这里**——`crates/shell/src/apps.rs` 把
// `weather-agent` 列进「桌面也默认 module 托管」那一档，shell 在自己进程里
// 建好 isolate，broker 由 `ai_host::offer` 注入进来。别改回进程托管：子进程
// 既拿不到那张注入表（进程内的静态量），也没法在同一个 core 目录上再跑一个
// 内核（一个目录只允许一个写者）。
//
// 这条 fallback **只**在 `attach_assistant` 拿到 None 时触发，已经拿到
// Some(broker) 的不走这里——shell 注入成功的话跟内核所有权不冲突。
use std::sync::{Arc, Mutex};

use makepad_widgets::log;
use octosense_app_peers::{hosted, injection, OctosAppService, OCTOS_SERVICES};
use octosense_kernel::{self, Connection};
use octosense_llm_config::profile;

/// 把 connection 留住，不让 kernel 子进程跟着进程退出。它活着 → 我们的
/// broker 一直能 dispatch；进程退出 → 自然 drop → kernel idle stop 走人。
static KEEP_ALIVE: Mutex<Option<Connection>> = Mutex::new(None);

/// 在 `attach_assistant` 拿到 None 的时候调用。返回一个 broker 给 Shell 用；
/// 起不来就返回 None，让控制台走「AI 助手未连接」错误提示。
pub fn bootstrap_if_needed(scope: &str) -> Option<Arc<dyn OctosAppService>> {
    // 1. 配置 + connect 起内核进程。
    let core_dir = profile::default_core_dir();
    let mut opts = octosense_kernel::Options::default().log(|line| log!("weather-kernel: {line}"));
    if let Some(dir) = core_dir {
        opts = opts.app_data_dir(dir);
    }
    octosense_kernel::configure(opts);
    let conn = match octosense_kernel::connect() {
        Ok(c) => c,
        Err(why) => {
            log!(
                "weather-agent fallback: kernel 未启动（{why}）；AI 控制台退化成错误提示"
            );
            return None;
        }
    };

    // 2. 留住 connection。进程退出时它自然 drop，kernel idle stop 走人。
    if let Ok(mut slot) = KEEP_ALIVE.lock() {
        *slot = Some(conn);
    }

    // 3. 本进程内的 policy——不需要过 ai_host 那套全局 OnceLock，自己 new 一份。
    let policy = Arc::new(hosted::HostPolicy::default());
    policy.allow("weather-agent", OCTOS_SERVICES);

    // 4. 起 broker，注入到 injection map 给本次 standalone scope 用。
    let broker = hosted::launch(
        "weather-agent",
        "天气助手",
        OCTOS_SERVICES,
        &policy,
    )?;
    hosted::offer("weather-agent", scope, &broker);

    // 5. 立刻 claim 出来——offer 的生命期是 `Offer::finish()` 之前，claim 之后
    // map 里就没了，不会有别人抢。
    injection::claim("weather-agent", scope)
}

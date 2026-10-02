// 诊断探针（不是回归测试）：走完 shell 给天气助手的那一整条 AI 链路，
// 对着本机真实的内核和真实的 profile 跑一遍，把每一步的结果打印出来。
//
// 只在显式要求时跑，因为它会启动真实内核并真的调用一次配置的模型：
//
//   set WEATHER_AI_PROBE=1
//   cargo test -p octosense-weather-agent --test probe_ai -- --nocapture --ignored
//
// 每一步都对应 shell 侧的一段代码：
//   step 1  octosense_kernel::configure/connect  —— ai_host::start 里那步
//   step 2  hosted::launch                       —— ai_host::offer 里那步
//   step 3  set_account + open_context           —— module.rs::create → AgentClient::new
//   step 4  service.model()                      —— 徽章标签的来源
//   step 5  ContextOp::Turn                      —— 控制台真的发一条消息
#![cfg(all(feature = "standalone", test))]

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use octosense_app_peers::{hosted, ContextSpec, OCTOS_SERVICES};

fn probe_enabled() -> bool {
    std::env::var_os("WEATHER_AI_PROBE").is_some()
}

fn services() -> BTreeSet<String> {
    OCTOS_SERVICES.iter().map(|s| s.to_string()).collect()
}

#[test]
#[ignore = "WEATHER_AI_PROBE=1 to run against the real kernel and model"]
fn the_hosted_assistant_path_works_end_to_end() {
    assert!(probe_enabled(), "set WEATHER_AI_PROBE=1");

    let services = services();

    // ---- step 1: 内核能不能起 -------------------------------------------------
    let core_dir = octosense_llm_config::profile::default_core_dir();
    let mut opts =
        octosense_kernel::Options::default().log(|line| println!("  [kernel] {line}"));
    if let Some(dir) = &core_dir {
        opts = opts.app_data_dir(dir);
    }
    octosense_kernel::configure(opts);
    println!("  core dir: {:?}", octosense_kernel::core_dir());
    println!("  launch: {:?}", octosense_kernel::launch());
    let conn = match octosense_kernel::connect() {
        Ok(c) => {
            println!("step 1 OK  connect()");
            c
        }
        Err(why) => {
            println!("step 1 FAIL connect(): {why}");
            std::process::exit(2);
        }
    };

    // ---- step 2: policy + broker --------------------------------------------
    let policy = std::sync::Arc::new(hosted::HostPolicy::default());
    policy.allow("weather-agent", OCTOS_SERVICES);
    let broker = match hosted::launch("weather-agent", "天气助手", OCTOS_SERVICES, &policy) {
        Some(b) => {
            println!("step 2 OK  launch()  services={:?}", b.config().services);
            b
        }
        None => {
            println!("step 2 FAIL launch() -> None（declared 与 policy 的交集是空的？）");
            std::process::exit(3);
        }
    };
    let service: std::sync::Arc<dyn octosense_app_peers::OctosAppService> =
        std::sync::Arc::new(broker.clone());

    // ---- step 3: account + context（AgentClient::new 的前两步）--------------
    service.set_account(Some("local"));
    let opened = service.open_context(ContextSpec {
        account: "local".into(),
        instance: "probe#1".into(),
        services: services.clone(),
    });
    println!(
        "step 3    open_context -> {}",
        match &opened {
            Ok(_) => "Ok",
            Err(e) => return println!("step 3 FAIL {e}"),
        }
    );
    let ctx = opened.unwrap();

    // ---- step 4: model（徽章标签的来源）-------------------------------------
    // set_account 只是把 ensure_peer 丢到后台线程，所以这里轮询一小会儿。
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut model = None;
    while Instant::now() < deadline {
        if let Some(m) = service.model() {
            model = Some(m);
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    match &model {
        Some(m) => println!("step 4 OK  model lane={} provider={:?} model={:?}", m.lane, m.provider, m.model),
        None => println!(
            "step 4 FAIL model() 一直是 None —— 说明 peer/prepare 没成功\n         availability = {:?}",
            service.availability()
        ),
    }

    // ---- step 5: 真的发一条消息 ---------------------------------------------
    use octosense_app_peers::ContextEvent;
    let text: std::sync::Arc<std::sync::Mutex<String>> = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let error: std::sync::Arc<std::sync::Mutex<String>> = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let start = Instant::now();
    let sink: octosense_app_peers::EventSink = std::sync::Arc::new({
        let text = text.clone();
        let error = error.clone();
        move |event| {
            match event {
                ContextEvent::Data(data) => {
                    if data.get("text").and_then(serde_json::Value::as_str).is_some() {
                        print!(".");
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }
                }
                ContextEvent::Complete(Ok(value)) => {
                    println!();
                    let got = value
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if !got.is_empty() {
                        *text.lock().unwrap() = got;
                    }
                }
                ContextEvent::Complete(Err(err)) => {
                    println!("\nstep 5 FAIL {err}");
                    *error.lock().unwrap() = err;
                }
            }
        }
    });
    match ctx.call(
        octosense_app_peers::ContextOp::Turn { text: "用一句话说你是谁。".into() },
        sink,
    ) {
        Ok(()) => println!("step 5    turn 已提交，等回话…"),
        Err(err) => println!("step 5 FAIL 提交失败：{err}"),
    }
    let mut done = false;
    let deadline = Instant::now() + Duration::from_secs(120);
    while !done && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
        done = !text.lock().unwrap().is_empty() || !error.lock().unwrap().is_empty();
    }
    let answer = text.lock().unwrap().clone();
    let turn_error = error.lock().unwrap().clone();
    println!("step 5    耗时 {:?}，回话 {answer}", start.elapsed());
    assert!(
        !answer.is_empty(),
        "AI 控制台那条链路没跑通：error={turn_error:?} availability={:?}",
        service.availability()
    );
    drop(conn);
    octosense_kernel::shutdown();
}

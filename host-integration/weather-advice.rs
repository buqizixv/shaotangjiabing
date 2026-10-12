use makepad_script::*;
use makepad_script::{parser::ScriptParser, tokenizer::ScriptTokenizer};
use octosense_llm_service::complete::{schema::Schema, Request};
use serde_json::{json, Value};

const SCRIPT: &str = include_str!("../bundle/main.splash");
const ENV: &str = r#"
let fs = {write: fn(path, data){}}
let widget = {render: fn(){}}
let ui = {content: widget}
fn round(value){ value }
let control = {set_text: fn(value){} set_visible: fn(value){}}
ui.schedule_mode_button = control
ui.clothing_mode_button = control
ui.preference_mode_button = control
ui.family_city_mode_button = control
ui.schedule_hint = control
let pending = []
let timers = []
fn start_timeout(seconds, callback){ timers.push(callback); timers.len() }
let host = {request: fn(method, args, callback){ pending.push({method: method args: args callback: callback}) }}
"#;
const WEATHER: &str = r#"{
    current: {temperature_2m: 18 apparent_temperature: 16 precipitation: 0 weather_code: 0}
    daily: {
        time: ["2026-10-04"] temperature_2m_min: [9] temperature_2m_max: [23]
        precipitation_probability_max: [65] uv_index_max: [5]
    }
}"#;

fn run(body: &str) -> Value {
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let logic = SCRIPT.split_once("\nSolidView{").expect("UI boundary").0;
    let value = vm.with_instruction_limit(4_000_000, |vm| vm.eval(ScriptMod {
        file: "weather_advice_test.splash".into(),
        code: format!("{ENV}\n{logic}\n{body}\n;"),
        ..Default::default()
    }));
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}: Splash errors executing {body}");
    assert!(!value.is_err(), "Splash returned {value:?}");
    let text = vm.bx.heap.string_with(value, |_, s| s.to_string()).expect("JSON result");
    serde_json::from_str(&text).expect(&text)
}

#[test]
fn complete_weather_page_parses() {
    let mut heap = ScriptHeap::default();
    let mut tokenizer = ScriptTokenizer::default();
    tokenizer.tokenize(SCRIPT, &mut heap);
    tokenizer.tokenize("\n;", &mut heap);
    let mut parser = ScriptParser::default();
    parser.set_emit_errors(false);
    parser.parse(&tokenizer, "weather_assistant.splash", (0, 0), &[]);
    assert!(!parser.had_error, "{:?}", parser.diagnostics());
}

#[test]
fn family_alerts_baseline_deduplicate_and_ignore_stale_weather() {
    let result = run(r#"
        let city = {name: "北京" country: "中国" admin1: "北京" latitude: 40 longitude: 116}
        observe_family(city, {current: {time: "2026-10-09T10:00" temperature_2m: 20 weather_code: 0 wind_speed_10m: 10}})
        let baseline_count = family_alerts.len()
        observe_family(city, {current: {time: "2026-10-09T11:00" temperature_2m: 14 weather_code: 61 wind_speed_10m: 45}})
        observe_family(city, {current: {time: "2026-10-09T11:00" temperature_2m: 14 weather_code: 61 wind_speed_10m: 45}})
        observe_family(city, {current: {time: "2026-10-09T09:00" temperature_2m: 30 weather_code: 0 wind_speed_10m: 5}})
        {baseline: baseline_count alerts: family_alerts weather: family_weather requests: pending.len()}.to_json()
    "#);
    assert_eq!(result["baseline"], 0);
    assert_eq!(result["alerts"].as_array().unwrap().len(), 1);
    let text = result["alerts"][0]["text"].as_str().unwrap();
    assert!(text.contains("开始降水") && text.contains("降温 6") && text.contains("40 km/h"));
    assert_eq!(result["weather"][0]["time"], "2026-10-09T11:00");
    assert_eq!(result["requests"], 0, "Rinx is opt-in");
}

#[test]
fn family_changes_use_each_city_and_respect_master_switch() {
    let result = run(r#"
        let a = {name: "北京" country: "中国" admin1: "北京" latitude: 40 longitude: 116}
        let b = {name: "上海" country: "中国" admin1: "上海" latitude: 31 longitude: 121}
        let clear = {current: {time: "2026-10-09T10:00" temperature_2m: 20 weather_code: 0 wind_speed_10m: 10}}
        observe_family(a, clear)
        observe_family(b, clear)
        reminders_on = false
        observe_family(b, {current: {time: "2026-10-09T11:00" temperature_2m: 14 weather_code: 61 wind_speed_10m: 45}})
        observe_family(a, {current: {time: "2026-10-09T11:00" temperature_2m: 21 weather_code: 0 wind_speed_10m: 11}})
        {alerts: family_alerts.len() weather: family_weather}.to_json()
    "#);
    assert_eq!(result["alerts"], 0);
    assert_eq!(result["weather"].as_array().unwrap().len(), 2);
}

#[test]
fn rinx_send_waits_for_actual_success_and_failure_can_retry() {
    let result = run(r#"
        rinx_room = "!home:matrix.rinx.chat"
        family_alerts = [{id: "a" city: "北京" text: "北京降雨" state: "ready" error: ""}]
        send_family_alert("a")
        let before = family_alerts[0].state
        send_family_alert("a")
        pending[0].callback({is_ok: false error: "The person declined to send."})
        let failed = family_alerts[0].state
        send_family_alert("a")
        pending[1].callback({is_ok: true data: {}})
        send_family_alert("a")
        {before: before failed: failed state: family_alerts[0].state calls: pending.len() method: pending[0].method args: pending[0].args}.to_json()
    "#);
    assert_eq!(result["before"], "sending");
    assert_eq!(result["failed"], "error");
    assert_eq!(result["state"], "sent");
    assert_eq!(result["calls"], 2);
    assert_eq!(result["method"], "storage.daycast.rinx.send");
    assert_eq!(result["args"], json!({"room":"!home:matrix.rinx.chat", "text":"北京降雨"}));
}

#[test]
fn rinx_enabled_changes_queue_behind_one_confirmation() {
    let result = run(r#"
        rinx_on = true
        rinx_room = "!home:matrix.rinx.chat"
        let a = {name: "北京" country: "中国" admin1: "北京" latitude: 40 longitude: 116}
        let b = {name: "上海" country: "中国" admin1: "上海" latitude: 31 longitude: 121}
        let clear = {current: {time: "2026-10-09T10:00" temperature_2m: 20 weather_code: 0 wind_speed_10m: 10}}
        let rain = {current: {time: "2026-10-09T11:00" temperature_2m: 20 weather_code: 61 wind_speed_10m: 10}}
        observe_family(a, clear)
        observe_family(b, clear)
        observe_family(a, rain)
        observe_family(b, rain)
        let in_flight = pending.len()
        pending[0].callback({is_ok: true data: {}})
        let after = pending.len()
        pending[1].callback({is_ok: true data: {}})
        {in_flight: in_flight after: after states: [family_alerts[0].state family_alerts[1].state]}.to_json()
    "#);
    assert_eq!(result["in_flight"], 1);
    assert_eq!(result["after"], 2);
    assert_eq!(result["states"], json!(["sent", "sent"]));
}

#[test]
fn family_extreme_thresholds_and_small_changes() {
    let result = run(r#"
        let normal = {temp: 30 wet: false wind: 10}
        {small: family_change(normal, {temp: 31 wet: false wind: 11})
         hot: family_change(normal, {temp: 35 wet: false wind: 10})
         cold: family_change({temp: 1 wet: false wind: 10}, {temp: 0 wet: false wind: 10})}.to_json()
    "#);
    assert_eq!(result["small"], "");
    assert!(result["hot"].as_str().unwrap().contains("防暑"));
    assert!(result["cold"].as_str().unwrap().contains("冰点"));
}

#[test]
fn missing_observation_fields_do_not_destroy_city_baselines() {
    let result = run(r#"
        let city = {name: "北京" country: "中国" admin1: "北京" latitude: 40 longitude: 116}
        observe_family(city, {})
        observe_family(city, {current: {time: "2026-10-09T10:00"}})
        {weather: family_weather.len() alerts: family_alerts.len()}.to_json()
    "#);
    assert_eq!(result, json!({"weather":0, "alerts":0}));
}

#[test]
fn legacy_storage_without_rinx_fields_remains_readable() {
    let result = run(r#"
        fs.exists = fn(path){true}
        fs.read = fn(path){"{\"current_city\":{\"name\":\"北京\",\"latitude\":40,\"longitude\":116},\"cities\":[],\"preferences\":[],\"family_cities\":[],\"wardrobe\":[],\"wardrobe_locations\":[],\"wardrobe_photos\":[],\"schedule\":[],\"reminders_on\":true}"}
        refresh_weather = fn(){}
        load_data()
        {room: rinx_room enabled: rinx_on weather: family_weather alerts: family_alerts}.to_json()
    "#);
    assert_eq!(result, json!({"room":"", "enabled":false, "weather":[], "alerts":[]}));
}

#[test]
fn enabling_rinx_leaves_old_drafts_manual() {
    let result = run(r#"
        rinx_room = "!home:matrix.rinx.chat"
        family_alerts = [{id: "old" city: "北京" text: "北京降雨" state: "ready" error: ""}]
        toggle_rinx()
        send_next_family_alert()
        {state: family_alerts[0].state calls: pending.len() enabled: rinx_on}.to_json()
    "#);
    assert_eq!(result, json!({"state":"manual", "calls":0, "enabled":true}));
}

#[test]
fn weather_bundle_has_model_access_and_current_digest() {
    use octosense_app_contract::{digest_dir, parse, resolve, HostLimits};
    let bundle = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../weather-assistant/bundle");
    let digest = digest_dir(&bundle).unwrap();
    let manifest: Value = serde_json::from_str(include_str!("../bundle/manifest.json")).unwrap();
    assert_eq!(manifest["integrity"]["bundle_blake3"], digest);
    let parsed = parse(&manifest.to_string()).unwrap();
    assert!(resolve(&parsed, &HostLimits::system()).unwrap().allows("model"));
}

#[test]
fn ai_request_contains_weather_and_selected_preferences() {
    let result = run(&format!(r#"
        current_weather = {WEATHER}
        preferences = ["怕冷" "关注降雨"]
        request_outing_advice()
        timers[0]()
        {{count: pending.len() method: pending[0].method args: pending[0].args}}.to_json()
    "#));
    assert_eq!(result["count"], 1);
    assert_eq!(result["method"], "model.complete");
    let args = &result["args"];
    Request::from_args(args).unwrap();
    let schema = Schema::compile(&args["schema"]).unwrap();
    schema.validate(&json!({"advice": "体感16℃，带件外套；今天有降水可能，带伞。"})).unwrap();
    assert_eq!(args["input"]["preferences"], json!(["怕冷", "关注降雨"]));
    assert_eq!(args["input"]["apparent_temperature_c"], 16);
    assert_eq!(args["input"]["daily_precipitation_probability_max_percent"], 65);
    assert_eq!(args["input"]["daily_uv_index_max"], 5);
}

#[test]
fn changed_preferences_discard_old_ai_reply() {
    let result = run(&format!(r#"
        current_weather = {WEATHER}
        preferences = ["怕冷"]
        request_outing_advice()
        timers[0]()
        toggle_preference("关注降雨")
        pending[0].callback({{is_ok: true data: {{output: {{advice: "旧建议"}}}}}})
        timers[2]()
        pending[1].callback({{is_ok: true data: {{output: {{advice: "带外套和雨伞"}}}}}})
        {{advice: advice_text preferences: preferences}}.to_json()
    "#));
    assert_eq!(result["advice"], "带外套和雨伞");
    assert_eq!(result["preferences"], json!(["怕冷", "关注降雨"]));
}

#[test]
fn weather_question_clears_input_and_reports_model_error() {
    let result = run(r#"
        let entered = "hi"
        let question = {text: fn(){entered} set_text: fn(value){entered = value}}
        ui.question = question
        send_weather_question()
        pending[0].callback({is_ok: false error: "unexpected argument '--host-managed' found"})
        {entered: entered request: pending[0].method prompt: chat_messages[0].text error: chat_messages[1].text}.to_json()
    "#);
    assert_eq!(result["entered"], "");
    assert_eq!(result["request"], "model.complete");
    assert_eq!(result["prompt"], "hi");
    assert_eq!(result["error"], "AI 暂时不可用，请检查 AI 设置后重试。");
}

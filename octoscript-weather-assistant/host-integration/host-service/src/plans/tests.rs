use super::*;
use crate::handle;
use std::sync::atomic::{AtomicU64, Ordering};

struct Store(std::path::PathBuf);
impl Store {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path=std::env::temp_dir().join(format!("daycast-plan-tests-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::SeqCst)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Store { fn drop(&mut self) { let _=std::fs::remove_dir_all(&self.0); } }
fn fixture() -> Value {
    json!({"plan_id":"travel-test-v1","confirmed":true,"context":{"weather_city":"杭州","forecast_dates":["2030-01-01"],"wardrobe":["外套"]},"plan":{
        "title":"西湖慢游","destination":"杭州 · 西湖","date":"2030-01-01","party":"2人","transport":"公交","budget_note":"300元上限",
        "summary":"上午户外，午后休息。","readiness_reason":"日期、时间、交通和雨天方式已明确。",
        "facts":[{"label":"预算","value":"300元","source":"user"}],"assumptions":["午餐按常规安排，可修改。"],
        "outfit":[{"item":"外套","reason":"随身体感增减。"}],"uses_forecast":true,"weather_city":"杭州","weather_summary":"提供的预报提示可能下雨。",
        "steps":[{"title":"散步","start":"2030-01-01T10:00","end":"2030-01-01T11:00","location":"西湖","details":"分段休息。"},
                 {"title":"返程","start":"2030-01-01T12:00","end":"2030-01-01T13:00","location":"公交","details":"预留缓冲。"}],
        "alternatives":[{"step_index":0,"condition":"降雨","title":"室内休息","details":"同一时段转室内，之后按原计划返程。"}]
    }})
}
#[test]
fn batches_only_write_after_confirmation_and_retry_is_idempotent() {
    let store=Store::new(); let mut args=fixture();args["confirmed"]=json!(false);
    assert!(handle(crate::APP,"daycast.plan.sync",&args,&store.0).is_err());
    assert!(!crate::store_path(&store.0).exists());
    let validation=handle(crate::APP,"daycast.plan.validate",&args,&store.0).unwrap();
    assert_eq!(validation["plan"]["steps"].as_array().unwrap().len(),2);
    assert!(!crate::store_path(&store.0).exists());
    args["confirmed"]=json!(true);
    let first=handle(crate::APP,"daycast.plan.sync",&args,&store.0).unwrap();
    assert_eq!(first["count"],2);assert_eq!(first["already_synced"],false);
    let bytes=std::fs::read(crate::store_path(&store.0)).unwrap();
    let retry=handle(crate::APP,"daycast.plan.sync",&args,&store.0).unwrap();
    assert_eq!(retry["already_synced"],true);assert_eq!(retry["events"].as_array().unwrap().len(),2);
    assert_eq!(std::fs::read(crate::store_path(&store.0)).unwrap(),bytes);
    assert!(retry["events"][0]["notes"].as_str().unwrap().contains("行程流程"));
    assert_eq!(retry["events"][0]["plan_snapshot"]["alternatives"][0]["title"],"室内休息");
}
#[test]
fn conflict_at_a_later_step_never_partially_adds_the_earlier_step() {
    let store=Store::new();
    handle(crate::APP,"daycast.schedule.add",&json!({"title":"已有安排","start":"2030-01-01T12:30","end":"2030-01-01T13:30"}),&store.0).unwrap();
    let before=std::fs::read(crate::store_path(&store.0)).unwrap();
    let err=handle(crate::APP,"daycast.plan.sync",&fixture(),&store.0).unwrap_err();
    assert!(err.contains("冲突"));assert_eq!(std::fs::read(crate::store_path(&store.0)).unwrap(),before);
}
#[test]
fn changed_or_partially_deleted_synced_plans_cannot_duplicate_events() {
    let store=Store::new();let args=fixture();
    handle(crate::APP,"daycast.plan.sync",&args,&store.0).unwrap();
    let mut changed=args.clone();changed["plan"]["steps"][0]["title"]=json!("换了活动");
    assert!(handle(crate::APP,"daycast.plan.sync",&changed,&store.0).is_err());
    handle(crate::APP,"daycast.schedule.remove",&json!({"id":"travel-test-v1-0"}),&store.0).unwrap();
    assert!(handle(crate::APP,"daycast.plan.sync",&args,&store.0).is_err());
    assert_eq!(crate::load(&store.0).unwrap().len(),1);
}
#[test]
fn invalid_time_or_internal_overlap_does_not_touch_storage() {
    let store=Store::new();let mut args=fixture();
    args["plan"]["steps"][0]["start"]=json!("2030-01-01T10:00:00");
    assert!(handle(crate::APP,"daycast.plan.sync",&args,&store.0).is_err());
    args=fixture();args["plan"]["steps"][1]["start"]=json!("2030-01-01T10:30");
    assert!(handle(crate::APP,"daycast.plan.sync",&args,&store.0).is_err());
    assert!(!crate::store_path(&store.0).exists());
}
#[test]
fn another_city_or_uncovered_forecast_cannot_be_claimed() {
    let mut args=fixture();args["context"]["weather_city"]=json!("太原");
    assert!(validate(&args).unwrap_err().contains("预报不匹配"));
    args=fixture();args["context"]["forecast_dates"]=json!(["2029-12-31"]);
    assert!(validate(&args).is_err());
    args["plan"]["uses_forecast"]=json!(false);args["plan"]["weather_summary"]=json!("虚构的晴天");
    let plan=validate(&args).unwrap();assert!(!plan.weather_summary.contains("晴天"));assert!(plan.weather_summary.contains("暂无"));
}
#[test]
fn nonexistent_clothes_and_unanchored_alternatives_are_rejected() {
    let mut args=fixture();args["plan"]["outfit"][0]["item"]=json!("衣橱中不存在的衣物");
    assert!(validate(&args).unwrap_err().contains("衣橱"));
    args=fixture();args["plan"]["alternatives"][0]["step_index"]=json!(99);
    assert!(validate(&args).is_err());
}
#[test]
fn both_daycast_ids_are_allowed_but_other_apps_cannot_sync() {
    let store=Store::new();
    assert!(handle("another-app","daycast.plan.sync",&fixture(),&store.0).is_err());
    assert!(handle("weather-assistant","daycast.plan.sync",&fixture(),&store.0).is_ok());
}
#[test]
fn legacy_events_keep_working_and_plan_notes_survive_later_event_writes() {
    let store=Store::new();let args=fixture();
    handle(crate::APP,"daycast.schedule.list",&json!({"legacy":[{"title":"旧日程","start":"2029-12-31T08:00","end":"2029-12-31T09:00"}]}),&store.0).unwrap();
    handle(crate::APP,"daycast.plan.sync",&args,&store.0).unwrap();
    handle(crate::APP,"daycast.schedule.add",&json!({"title":"新日程","start":"2030-01-02T08:00","end":"2030-01-02T09:00"}),&store.0).unwrap();
    let stored=crate::load(&store.0).unwrap();assert_eq!(stored.len(),4);
    assert!(stored.iter().find(|e|e.id=="travel-test-v1-0").unwrap().notes.as_ref().unwrap().contains("外套"));
}

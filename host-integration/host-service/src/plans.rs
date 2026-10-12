//! Validation and all-or-nothing calendar writes for a reviewed travel plan.
//! Model output is a proposal. Only the explicit UI sync call writes events.
use crate::{Event, EVENTS_MAX};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact { pub label: String, pub value: String, pub source: String }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outfit { pub item: String, pub reason: String }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyOutfit { pub date: String, pub items: Vec<Outfit> }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackingItem { pub item: String, pub quantity: u32, pub source: String, pub reason: String }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step { pub title: String, pub start: String, pub end: String, pub location: String, pub details: String }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alternative { pub step_index: usize, pub condition: String, pub title: String, pub details: String, #[serde(default)] pub rule: Option<WeatherRule> }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherRule { pub metric: String, pub operator: String, pub threshold: f64 }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub title: String, pub destination: String, pub date: String,
    pub party: String, pub transport: String, pub budget_note: String,
    pub summary: String, pub readiness_reason: String,
    pub facts: Vec<Fact>, pub assumptions: Vec<String>, pub outfit: Vec<Outfit>,
    pub uses_forecast: bool, pub weather_city: String, pub weather_summary: String,
    pub steps: Vec<Step>, pub alternatives: Vec<Alternative>,
    #[serde(default)] pub daily_outfits: Vec<DailyOutfit>,
    #[serde(default)] pub packing: Vec<PackingItem>,
}

fn bounded(value: &str, name: &str, max: usize, required: bool) -> Result<(), String> {
    if value.chars().count() > max || (required && value.trim().is_empty()) {
        return Err(format!("{name}内容不完整或过长，请重新生成。"));
    }
    Ok(())
}
fn strict_time(value: &str) -> Result<NaiveDateTime, String> {
    let parsed = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .map_err(|_| "行程时间必须是完整的 YYYY-MM-DDTHH:mm。".to_string())?;
    if parsed.format("%Y-%m-%dT%H:%M").to_string() != value {
        return Err("行程时间必须是完整的 YYYY-MM-DDTHH:mm。".into());
    }
    Ok(parsed)
}
pub(crate) fn validate(args: &Value) -> Result<Plan, String> {
    let mut readiness=args["context"]["readiness_input"].clone();
    readiness["now"]=json!(Local::now().timestamp() as f64);
    let assessment = crate::confidence::assess(&readiness)?;
    if assessment["ready"] != true { return Err("关键信息或目的地天气尚未核实，不能宣称有把握；请先完成缺口确认。".into()); }
    let mut plan: Plan = serde_json::from_value(args.get("plan").cloned().unwrap_or(Value::Null))
        .map_err(|_| "AI 返回的计划结构不完整，请重试。".to_string())?;
    for (v, n, max, required) in [
        (&plan.title, "计划标题", 80, true), (&plan.destination, "目的地", 100, true),
        (&plan.party, "同行安排", 80, true), (&plan.transport, "交通方式", 120, true),
        (&plan.budget_note, "预算说明", 160, false), (&plan.summary, "行程摘要", 800, true),
        (&plan.readiness_reason, "生成依据", 400, true), (&plan.weather_city, "天气地点", 100, false),
        (&plan.weather_summary, "天气说明", 400, false),
    ] { bounded(v, n, max, required)?; }
    let date = NaiveDate::parse_from_str(&plan.date, "%Y-%m-%d").map_err(|_| "请明确出行日期。".to_string())?;
    let (first,last)=crate::confidence::dates(crate::confidence::value(&assessment,"dates")).ok_or("请先确认出行起止日期。")?;
    if date!=first || !plan.destination.contains(crate::confidence::value(&assessment,"destination")) {return Err("生成的目的地或日期与用户确认的安排不一致。".into());}
    plan.party=crate::confidence::value(&assessment,"party").to_owned();
    bounded(&plan.party,"同行安排",80,true)?;
    plan.facts=assessment["known"].as_array().unwrap().iter().map(|row|Fact{label:row["label"].as_str().unwrap_or("").into(),value:row["value"].as_str().unwrap_or("").into(),source:row["source"].as_str().unwrap_or("user").into()}).collect();
    plan.readiness_reason=format!("出行日期、目的地、出发地、同行、节奏与天气偏好已有依据。{}；{}。交通班次、营业信息仍需出发前核对。",assessment["weather"].as_str().unwrap_or(""),assessment["calendar"].as_str().unwrap_or(""));
    if date.format("%Y-%m-%d").to_string() != plan.date { return Err("请明确完整的出行日期。".into()); }
    if plan.steps.is_empty() || plan.steps.len() > 12 || plan.alternatives.len() > 6
        || plan.facts.len() > 8 || plan.assumptions.len() > 8 || plan.outfit.len() > 6 {
        return Err("计划应包含 1–12 项行程，其他内容请保持简洁。".into());
    }
    for fact in &plan.facts {
        bounded(&fact.label, "需求名称", 40, true)?; bounded(&fact.value, "需求", 160, true)?;
        if !["user", "profile", "default"].contains(&fact.source.as_str()) { return Err("计划需要标明需求来源。".into()); }
    }
    for assumption in &plan.assumptions { bounded(assumption, "默认安排", 240, true)?; }
    let wardrobe = args.pointer("/context/wardrobe").and_then(Value::as_array);
    for item in &plan.outfit {
        bounded(&item.item, "衣物", 80, true)?; bounded(&item.reason, "穿搭理由", 200, true)?;
        if !wardrobe.is_some_and(|rows| rows.iter().any(|v| v.as_str() == Some(item.item.as_str()))) {
            return Err(format!("衣橱中没有“{}”，请只使用实际记录的衣物重新搭配。", item.item));
        }
    }
    let mut previous_end = None;
    for step in &plan.steps {
        bounded(&step.title, "行程标题", 80, true)?; bounded(&step.location, "行程地点", 120, false)?;
        bounded(&step.details, "行程说明", 600, false)?;
        let start = strict_time(&step.start)?; let end = strict_time(&step.end)?;
        if end <= start { return Err("每项行程的结束时间必须晚于开始时间。".into()); }
        if start.date() < first || end.date() > last {
            return Err("行程必须位于用户确认的出行起止日期内。".into());
        }
        if previous_end.is_some_and(|previous| start < previous) { return Err("行程内部时间重叠或顺序错误，请调整。".into()); }
        previous_end = Some(end);
    }
    for alternative in &mut plan.alternatives {
        if alternative.step_index >= plan.steps.len() { return Err("天气备选没有对应的行程。".into()); }
        bounded(&alternative.condition, "备选条件", 160, true)?;
        bounded(&alternative.title, "备选安排", 80, true)?; bounded(&alternative.details, "备选说明", 600, true)?;
        let rule=alternative.rule.as_ref().ok_or("天气备选需要可检测的天气规则。")?;
        if !["rain_probability_percent","temperature_min_c","temperature_max_c","weather_code"].contains(&rule.metric.as_str()) || !["gte","lte","eq"].contains(&rule.operator.as_str()) || !rule.threshold.is_finite() || rule.threshold < -100.0 || rule.threshold > 100.0 {return Err("天气规则无效。".into());}
        let (label,unit)=match rule.metric.as_str(){"rain_probability_percent"=>("降雨概率","%"),"temperature_min_c"=>("最低温","°C"),"temperature_max_c"=>("最高温","°C"),_=>("天气代码","")};
        let operator=match rule.operator.as_str(){"gte"=>"≥","lte"=>"≤",_=>"="};
        alternative.condition=format!("{label} {operator} {}{unit}",rule.threshold);
    }
    if plan.daily_outfits.len()>14 || plan.packing.len()>30 {return Err("每日穿搭或打包清单过长。".into());}
    for day in 0..=(last-first).num_days() {
        let day=(first+Duration::days(day)).to_string();
        let outfit=plan.daily_outfits.iter().find(|o|o.date==day).ok_or("每个出行日期都需要每日穿搭说明。")?;
        if outfit.items.len()>8 {return Err("单日穿搭数量过多。".into());}
        for item in &outfit.items {
            bounded(&item.reason,"每日穿搭理由",200,true)?;
            if !wardrobe.is_some_and(|w|w.iter().any(|v|v.as_str()==Some(item.item.as_str()))) {return Err("每日穿搭只能映射实际衣橱中的衣物。".into());}
        }
    }
    if plan.daily_outfits.len()!=(last-first).num_days() as usize+1 {return Err("每日穿搭日期重复或超出出行范围。".into());}
    if plan.packing.is_empty() {return Err("完整出行套餐需要打包清单。".into());}
    for item in &plan.packing {
        bounded(&item.item,"打包物品",80,true)?;bounded(&item.reason,"打包说明",200,true)?;
        if item.quantity==0 || item.quantity>30 || !["wardrobe","prepare"].contains(&item.source.as_str()) {return Err("打包数量或物品来源无效。".into());}
        if item.source=="wardrobe" && !wardrobe.is_some_and(|w|w.iter().any(|v|v.as_str()==Some(item.item.as_str()))) {return Err("不能把未记录的物品标为衣橱实物。".into());}
    }
    // A model cannot treat another city's weather or an uncovered date as
    // the forecast for this journey. Unknown weather remains explicit.
    let location = args.pointer("/context/weather_city").and_then(Value::as_str).unwrap_or("");
    let forecast_dates = args.pointer("/context/forecast_dates").and_then(Value::as_array);
    let covers_trip = plan.steps.iter().all(|step| {
        [step.start.get(..10), step.end.get(..10)].iter().all(|date| {
            forecast_dates.is_some_and(|rows| rows.iter().any(|row| row.as_str() == *date))
        })
    });
    if plan.uses_forecast && (location.is_empty() || location != plan.weather_city || !covers_trip) {
        return Err("目的地或日期与提供的预报不匹配，请核对天气范围后重新生成。".into());
    }
    if !plan.uses_forecast {
        plan.weather_summary = "目的地或出行日期暂无已核实的对应预报；出发前重新核对天气，并按备选安排调整。".into();
        plan.weather_city.clear();
    }
    if plan.alternatives.is_empty() { return Err("请为天气变化补充至少一条可执行的备选安排。".into()); }
    Ok(plan)
}

fn conflicts(plan: &Plan, events: &[Event]) -> Result<(), String> {
    for step in &plan.steps {
        if let Some(event) = events.iter().find(|event| event.start < step.end && step.start < event.end) {
            return Err(format!("“{}”与已有日程“{}”冲突，请调整后再同步。", step.title, event.title));
        }
    }
    Ok(())
}
pub(crate) fn validate_reply(args: &Value, events: &[Event]) -> Result<Value, String> {
    let plan = validate(args)?;
    let replace=args["replace_plan_id"].as_str().unwrap_or("");
    preserve_past(&plan,events,replace)?;
    let retained:Vec<_>=events.iter().filter(|e|replace.is_empty() || e.plan_id.as_deref()!=Some(replace)).cloned().collect();
    conflicts(&plan, &retained)?;
    system_conflicts(&plan, &args["context"]["readiness_input"]["events"])?;
    Ok(json!({"plan": plan}))
}
fn preserve_past(plan:&Plan,events:&[Event],replace:&str)->Result<(),String>{
    if replace.is_empty(){return Ok(());}
    let now=Local::now().format("%Y-%m-%dT%H:%M").to_string();
    for e in events.iter().filter(|e|e.plan_id.as_deref()==Some(replace) && e.end<=now) {
        if !plan.steps.iter().any(|s|s.title==e.title && s.start==e.start && s.end==e.end) {return Err("已完成的行程必须保留；只能调整尚未完成的安排。".into());}
    }
    Ok(())
}
fn system_conflicts(plan: &Plan, events: &Value) -> Result<(),String> {
    for step in &plan.steps {if events.as_array().is_some_and(|es|es.iter().any(|e|e["start"].as_str().unwrap_or("")<step.end.as_str() && step.start.as_str()<e["end"].as_str().unwrap_or(""))) {return Err("行程与已盘点日历冲突，请调整。".into());}}
    Ok(())
}
fn notes(plan: &Plan, step_index: usize) -> String {
    let step = &plan.steps[step_index];
    let mut text = format!("{}\n{}\n地点：{}\n{}\n天气：{}\n生成依据：{}\n", plan.title, plan.summary, step.location, step.details, plan.weather_summary, plan.readiness_reason);
    for item in &plan.outfit { text.push_str(&format!("穿搭：{} · {}\n", item.item, item.reason)); }
    for day in &plan.daily_outfits {for item in &day.items {text.push_str(&format!("{} 穿搭：{} · {}\n",day.date,item.item,item.reason));}}
    for item in &plan.packing {text.push_str(&format!("打包：{} × {}（{}）· {}\n",item.item,item.quantity,item.source,item.reason));}
    for item in &plan.assumptions { text.push_str(&format!("默认安排：{item}\n")); }
    for alternative in &plan.alternatives {
        text.push_str(&format!("天气备选（{}）：{} → {} · {}\n", plan.steps[alternative.step_index].title, alternative.condition, alternative.title, alternative.details));
    }
    text.push_str("行程流程：");
    text.push_str(&plan.steps.iter().map(|step| format!("{} {}", &step.start[11..], step.title)).collect::<Vec<_>>().join(" → "));
    text
}
fn sync_system(plan: &Plan,id:&str,host_dir:&Path,replace:&str)->Result<(),String>{
    let calendar_notes=|i:usize|{
        let step=&plan.steps[i];
        let mut text=format!("{}\n{}\n{}\n天气：{}\n",plan.title,plan.summary,step.details,plan.weather_summary);
        for day in &plan.daily_outfits {if step.start.starts_with(&day.date) {for item in &day.items {text.push_str(&format!("穿搭：{} · {}\n",item.item,item.reason));}}}
        for item in &plan.packing {text.push_str(&format!("打包：{} × {}（{}）\n",item.item,item.quantity,if item.source=="wardrobe" {"衣橱已有"} else {"需要准备"}));}
        for alt in &plan.alternatives {text.push_str(&format!("天气分支：{} → {} · {}\n",alt.condition,alt.title,alt.details.chars().take(120).collect::<String>()));}
        text.push_str("行程流程：\n");
        for s in &plan.steps {text.push_str(&format!("{}–{} {} · {}\n",s.start,s.end,s.title,s.location));}
        text.push_str("完整计划与天气分支详情保存在 Daycast 出行 Plan。\n");text
    };
    let rows:Vec<_>=plan.steps.iter().enumerate().map(|(i,s)|octosense_calendar_service::Event{
        id:format!("daycast-{id}-{i}"),title:s.title.clone(),start:s.start.clone(),end:Some(s.end.clone()),location:s.location.clone(),notes:calendar_notes(i)
    }).collect();
    octosense_calendar_service::travel::sync_replacing(host_dir,id,&rows,true,replace)
}
pub(crate) fn sync(args: &Value, events: &mut Vec<Event>, host_dir: &Path) -> Result<Value, String> {
    if args.get("confirmed").and_then(Value::as_bool) != Some(true) { return Err("请先确认要同步的完整行程。".into()); }
    let id = args.get("plan_id").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() || id.len() > 100 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err("计划标识无效，请重新生成。".into());
    }
    let plan = validate(args)?;
    let replace=args["replace_plan_id"].as_str().unwrap_or("");
    preserve_past(&plan,events,replace)?;
    let existing: Vec<_> = events.iter().filter(|event| event.plan_id.as_deref() == Some(id)).collect();
    if !existing.is_empty() {
        let matches = existing.len() == plan.steps.len() && plan.steps.iter().enumerate().all(|(i, step)| {
            existing.iter().any(|event| event.id == format!("{id}-{i}") && event.title == step.title && event.start == step.start && event.end == step.end
                && event.notes.as_deref() == Some(notes(&plan, i).as_str()) && event.plan_snapshot.as_ref() == Some(&plan))
        });
        if !matches { return Err("这份计划曾同步后被修改或删除，请查看日程后创建新版本。".into()); }
        sync_system(&plan,id,host_dir,replace)?;
        return Ok(json!({"events": events, "count": plan.steps.len(), "already_synced": true}));
    }
    if !replace.is_empty() && (replace==id || !events.iter().any(|e|e.plan_id.as_deref()==Some(replace))) {return Err("原行程已不存在，请核对日历后重新创建。".into());}
    let mut updated:Vec<_> = events.iter().filter(|e|replace.is_empty() || e.plan_id.as_deref()!=Some(replace)).cloned().collect();
    if updated.len() + plan.steps.len() > EVENTS_MAX { return Err("日程容量不足，整份行程尚未保存。".into()); }
    conflicts(&plan, &updated)?;
    sync_system(&plan,id,host_dir,replace)?;
    for (i, step) in plan.steps.iter().enumerate() {
        updated.push(Event { id: format!("{id}-{i}"), title: step.title.clone(), start: step.start.clone(), end: step.end.clone(),
            notes: Some(notes(&plan, i)), plan_id: Some(id.to_string()), plan_snapshot: Some(plan.clone()) });
    }
    updated.sort_by(|a,b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    // One validated write, never one request per step. A retry is idempotent.
    crate::save(host_dir, &updated).map_err(|e|format!("系统日历已同步，Daycast 附件保存未完成；请确认重试以修复，不会重复添加。{e}"))?;
    *events = updated;
    Ok(json!({"events": events, "count": plan.steps.len(), "already_synced": false}))
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().ok_or("无效的日程路径")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建日程目录：{e}"))?;
    let temp = dir.join(format!(".schedule-{}-{}.tmp", std::process::id(), Local::now().timestamp_nanos_opt().unwrap_or_default()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| format!("无法保存日程：{e}"))?;
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("无法保存日程：{e}"))?;
        drop(file);
        #[cfg(windows)]
        if path.exists() {
            use std::os::windows::ffi::OsStrExt;
            #[link(name="kernel32")]
            extern "system" { fn ReplaceFileW(replaced:*const u16,replacement:*const u16,backup:*const u16,flags:u32,exclude:*mut std::ffi::c_void,reserved:*mut std::ffi::c_void)->i32; }
            let old:Vec<_>=path.as_os_str().encode_wide().chain(Some(0)).collect();
            let new:Vec<_>=temp.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: these NUL-terminated buffers live through the call;
            // optional pointer parameters are null, as documented by Win32.
            if unsafe { ReplaceFileW(old.as_ptr(),new.as_ptr(),std::ptr::null(),0,std::ptr::null_mut(),std::ptr::null_mut()) } == 0 {
                return Err(format!("无法替换日程，原文件已保留：{}",std::io::Error::last_os_error()));
            }
            return Ok(());
        }
        std::fs::rename(&temp,path).map_err(|e| format!("无法保存日程，原文件已保留：{e}"))
    })();
    if result.is_err() { let _=std::fs::remove_file(temp); }
    result
}

#[cfg(test)]
mod tests;

//! Readiness is evidence coverage, never a model's self-reported probability.
use chrono::{Datelike, Duration, Local, NaiveDate};
use serde_json::{json, Value};

const SLOTS: [(&str, &str, u64); 6] = [
    ("destination", "目的地", 15), ("dates", "出行日期", 20),
    ("origin", "出发地", 15), ("party", "同行安排", 10),
    ("style", "节奏与预算", 10), ("sensitivity", "天气敏感度", 10),
];
fn string<'a>(v: &'a Value, key: &str) -> &'a str { v[key].as_str().unwrap_or("").trim() }
pub(crate) fn value<'a>(assessment: &'a Value, key: &str) -> &'a str {
    assessment["known"].as_array().and_then(|rows| rows.iter().find(|r| r["key"] == key))
        .map(|r| string(r, "value")).unwrap_or("")
}
pub(crate) fn dates(v: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (a,b) = v.split_once(" 至 ")?;
    let a = NaiveDate::parse_from_str(a, "%Y-%m-%d").ok()?;
    let b = NaiveDate::parse_from_str(b, "%Y-%m-%d").ok()?;
    (a <= b && b - a < Duration::days(14)).then_some((a,b))
}
pub(crate) fn assess(args: &Value) -> Result<Value, String> {
    let today = args["today"].as_str().and_then(|s|NaiveDate::parse_from_str(s,"%Y-%m-%d").ok())
        .unwrap_or_else(||Local::now().date_naive());
    let messages = args["messages"].as_array().ok_or("缺少用户对话依据，不能判断有把握。")?;
    let answers = args["answers"].as_array().ok_or("缺少确认记录。")?;
    let evidence = args["evidence"].as_array().ok_or("缺少信息盘点结果。")?;
    let mut known = vec![]; let mut missing = vec![]; let mut score = 0;
    for (key,label,weight) in SLOTS {
        // Answers are captured by the UI, outside model output. A remembered
        // profile only settles reusable preferences, never dates/companions.
        let explicit = answers.iter().rev().find(|a|string(a,"key")==key && !string(a,"value").is_empty()
            && !["随便","不知道","你决定","默认","都行","暂不确定","带孩子或长辈（请补充人数和限制）"].contains(&string(a,"value")));
        let extracted = evidence.iter().rev().find(|e| {
            let quote=string(e,"quote"); let v=string(e,"value");
            string(e,"key")==key && !v.is_empty() && !quote.is_empty()
                && messages.iter().any(|m|m["role"]=="user" && string(m,"content").contains(quote))
                && if key=="dates" {
                    // A vague weekend can NEVER count as a confirmed date.
                    dates(v).is_some_and(|(a,b)|quote.contains(&a.to_string()) && quote.contains(&b.to_string()))
                } else { quote.contains(v) && (key!="origin" || ["从","出发","住在","家在"].iter().any(|word|quote.contains(word))) }
        });
        let profile = if key=="sensitivity" { args["preferences"].as_array().filter(|v|!v.is_empty()).map(|p|p.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("、")) }
            else if key=="style" { args["profile_style"].as_str().filter(|v|!v.is_empty()).map(str::to_owned) } else {None};
        let picked=match (explicit,extracted) {
            (Some(a),Some(e)) if messages.iter().rposition(|m|m["role"]=="user" && string(m,"content").contains(string(e,"quote"))).unwrap_or(0) > messages.iter().rposition(|m|m["role"]=="user" && string(m,"content")==string(a,"value")).unwrap_or(0) => Some(e),
            (Some(a),_)=>Some(a),(_,e)=>e,
        };
        let found = picked.map(|r|(string(r,"value").to_owned(),"user"))
            .or_else(||profile.map(|p|(p,"profile")));
        if let Some((v,source))=found.filter(|(v,_)|key!="dates" || dates(v).is_some_and(|(_,b)|b>=today)) {
            known.push(json!({"key":key,"label":label,"value":v,"source":source})); score+=weight;
        } else { missing.push(json!({"key":key,"label":label})); }
    }
    let mut result=json!({"known":known,"missing":missing,"score":score,"threshold":90,"ready":false,"question":"","choices":[],"question_key":"","weather":"待查询","calendar":"待核对"});
    let destination=value(&result,"destination").to_owned();
    let range=dates(value(&result,"dates"));
    let weather=&args["weather"];
    let fresh=weather["fetched_at"].as_f64().zip(args["now"].as_f64()).is_some_and(|(a,b)|b>=a && b-a<21600.0);
    let covered=range.is_some_and(|(a,b)| (0..=(b-a).num_days()).all(|i|weather["dates"].as_array().is_some_and(|ds|ds.iter().any(|d|d.as_str()==Some((a+Duration::days(i)).to_string().as_str())))));
    let complete=range.is_some_and(|(a,b)| (0..=(b-a).num_days()).all(|i| {
        let date=(a+Duration::days(i)).to_string();
        weather["daily"]["time"].as_array().and_then(|ds|ds.iter().position(|d|d.as_str()==Some(date.as_str())))
            .is_some_and(|n|["temperature_2m_min","temperature_2m_max","precipitation_probability_max","weather_code"].iter().all(|key|weather["daily"][key][n].as_f64().is_some()))
    }));
    let forecast_ok=fresh && covered && complete && !destination.is_empty() && string(weather,"destination")==destination;
    let rules_only=answers.iter().any(|a|string(a,"key")=="weather_consent" && string(a,"value")=="先做待核实方案，出发前重检天气");
    if forecast_ok { score+=10; result["weather"]=json!("目的地逐日预报已核实"); }
    else if rules_only {score+=10;result["weather"]=json!("你已选择待核实方案；出发前必须重检");}
    if args["calendar_checked"]==true {score+=10;result["calendar"]=json!("系统日历与 Daycast 日程已盘点");}
    result["score"]=json!(score);
    let gap=result["missing"][0]["key"].as_str().unwrap_or("").to_owned();
    let (key,question,choices)=match gap.as_str() {
        "destination" => (gap,"这次想去哪个城市？请输入目的地。".to_owned(),vec![]),
        "dates" => {
            let mut choices=vec![];
            for i in 0..29 {
                let d=today+Duration::days(i);
                if d.weekday().number_from_monday()==6 {
                    let end=d+Duration::days(1);
                    let occupied=args["events"].as_array().is_some_and(|es|es.iter().any(|e|string(e,"start")<format!("{}T00:00",end+Duration::days(1)).as_str() && string(e,"end")>format!("{d}T00:00").as_str()));
                    if !occupied && choices.len()<3 {choices.push(format!("{d} 至 {end}"));}
                }
            }
            (gap,"你说的是任选一个周末，还没有确定日期。下面是日历没有安排的周末；选一个，或输入其他起止日期。".into(),choices)
        },
        "origin" => (gap,"从哪里出发？当前天气城市不代表你的出发地。".into(),args["current_city"].as_str().filter(|s|!s.is_empty()).map(|s|vec![s.to_owned()]).unwrap_or_default()),
        "party" => (gap,"这次和谁一起？同行会影响步行强度、房间和交通安排。".into(),vec!["独自出行".into(),"与一位朋友同行".into(),"带孩子或长辈（请补充人数和限制）".into()]),
        "style" => (gap,"想以什么节奏、预算来安排？也可以输入自己的上限和不能接受的安排。".into(),vec!["轻松慢游，优先免费景点，每人每天500元以内".into(),"适中节奏，每人每天1000元以内".into(),"密集探索，每人每天1500元以内".into()]),
        "sensitivity" => (gap,"天气方面有什么需要避开的？已有偏好会自动沿用。".into(),vec!["怕冷，避雨，减少户外等待".into(),"怕热，避免暴晒，优先有空调的地点".into(),"普通天气可接受，极端天气改为室内".into()]),
        _ if !forecast_ok && !rules_only => ("weather_consent".into(),"目的地或出行日期没有可用的实时预报。你想先保留进度，还是明确接受一份待核实方案？".into(),vec!["先做待核实方案，出发前重检天气".into()]),
        _ => ("".into(),"".into(),vec![]),
    };
    result["question_key"]=json!(key);result["question"]=json!(question);result["choices"]=json!(choices);
    result["ready"]=json!(result["missing"].as_array().unwrap().is_empty() && (forecast_ok||rules_only) && args["calendar_checked"]==true && score>=90);
    Ok(result)
}

#[cfg(test)] mod tests {
    use super::*;
    fn input()->Value {json!({"today":"2026-10-11","now":1000,"messages":[{"role":"user","content":"我想随便找一个周末去北京"}],"answers":[],"evidence":[{"key":"destination","value":"北京","quote":"去北京"}],"preferences":["怕冷"],"events":[],"calendar_checked":true,"weather":{}})}
    #[test] fn vague_weekend_never_becomes_nearest_weekend(){let mut a=input();a["evidence"].as_array_mut().unwrap().push(json!({"key":"dates","value":"2026-10-17 至 2026-10-18","quote":"随便找一个周末"}));let r=assess(&a).unwrap();assert_eq!(r["ready"],false);assert_eq!(r["question_key"],"dates");assert_eq!(r["choices"][0],"2026-10-17 至 2026-10-18");assert_eq!(value(&r,"origin"),"");}
    #[test] fn fabricated_quote_or_default_cannot_fill_a_gap(){let mut a=input();a["evidence"].as_array_mut().unwrap().push(json!({"key":"origin","value":"天津","quote":"去北京"}));assert_eq!(value(&assess(&a).unwrap(),"origin"),"");}
    #[test] fn calendar_conflicts_remove_weekend_choices(){let mut a=input();a["events"]=json!([{"start":"2026-10-18T11:00","end":"2026-10-18T12:00"}]);assert_eq!(assess(&a).unwrap()["choices"][0],"2026-10-24 至 2026-10-25");}
    #[test] fn complete_evidence_requires_fresh_destination_forecast(){let mut a=input();a["answers"]=json!([{"key":"dates","value":"2026-10-17 至 2026-10-18"},{"key":"origin","value":"天津"},{"key":"party","value":"独自出行"},{"key":"style","value":"轻松，500元"}]);a["weather"]=json!({"destination":"北京","fetched_at":999,"dates":["2026-10-17","2026-10-18"],"daily":{"time":["2026-10-17","2026-10-18"],"temperature_2m_min":[10,11],"temperature_2m_max":[20,21],"precipitation_probability_max":[20,60],"weather_code":[2,61]}});assert_eq!(assess(&a).unwrap()["ready"],true);a["weather"]["destination"]=json!("天津");assert_eq!(assess(&a).unwrap()["ready"],false);a["weather"]["destination"]=json!("北京");a["now"]=json!(30000);assert_eq!(assess(&a).unwrap()["ready"],false);}
}

//! Compare the saved proposal's weather rules. No model call or calendar write.
use crate::plans::{Plan, WeatherRule};
use serde_json::{json, Value};
fn hit(rule:&WeatherRule, day:&str, forecast:&Value)->Option<bool>{
    let daily=&forecast["data"]["daily"];
    let i=daily["time"].as_array()?.iter().position(|d|d.as_str()==Some(day))?;
    let field=match rule.metric.as_str(){"rain_probability_percent"=>"precipitation_probability_max","temperature_min_c"=>"temperature_2m_min","temperature_max_c"=>"temperature_2m_max","weather_code"=>"weather_code",_=>return None};
    let v=daily[field][i].as_f64()?;
    Some(match rule.operator.as_str(){"gte"=>v>=rule.threshold,"lte"=>v<=rule.threshold,"eq"=>v==rule.threshold,_=>return None})
}
pub(crate) fn recheck(args:&Value)->Result<Value,String>{
    let plan:Plan=serde_json::from_value(args["plan"].clone()).map_err(|_|"无法读取需要重检的计划。")?;
    if args["current"]["destination"].as_str()!=args["baseline"]["destination"].as_str() || !plan.destination.contains(args["current"]["destination"].as_str().filter(|s|!s.is_empty()).ok_or("缺少目的地天气。")?) {return Err("重检天气必须是本次目的地。".into());}
    let mut changes=vec![];
    let now=chrono::Local::now().format("%Y-%m-%dT%H:%M").to_string();
    for alt in &plan.alternatives {
        let Some(rule)=&alt.rule else {continue};
        let step=plan.steps.get(alt.step_index).ok_or("天气分支缺少对应步骤。")?;
        if step.end<=now {continue;}
        let date=step.start.get(..10).ok_or("行程日期无效。")?;
        let old=hit(rule,date,&args["baseline"]);let new=hit(rule,date,&args["current"]);
        if old!=new || new.is_none(){changes.push(json!({"date":date,"step":step.title,"condition":alt.condition,"alternative":alt.title,"triggered":new,"was_triggered":old}));}
    }
    Ok(json!({"needs_review":!changes.is_empty(),"changes":changes,"writes":0}))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn rule_crossings_and_missing_data_are_not_sunny(){let r=WeatherRule{metric:"rain_probability_percent".into(),operator:"gte".into(),threshold:60.0};let f=json!({"data":{"daily":{"time":["2026-10-17"],"precipitation_probability_max":[65]}}});assert_eq!(hit(&r,"2026-10-17",&f),Some(true));assert_eq!(hit(&r,"2026-10-18",&f),None);}
}

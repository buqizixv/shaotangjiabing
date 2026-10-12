//! Host-owned QWeather GeoAPI. These are the only two external paths this
//! service can request; callers cannot supply a URL or an API path.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, io::Read, path::PathBuf, sync::{Mutex, OnceLock}, time::{Duration, Instant}};

const CITY: &str = "/geo/v2/city/lookup";
const SCENIC: &str = "/geo/v2/poi/lookup";
const MAX_BODY: u64 = 512 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Deserialize)]
struct Config {
    api_host: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    jwt: String,
}

fn config_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("QWEATHER_CONFIG") { return Ok(path.into()); }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
        .ok_or("和风天气尚未配置，请在宿主设置 API Host 和认证信息")?;
    Ok(PathBuf::from(home).join(".config/octosense/qweather/config.json"))
}

impl Config {
    fn load() -> Result<Self, String> {
        let host = std::env::var("QWEATHER_API_HOST").unwrap_or_default();
        let config = if !host.is_empty() {
            Self { api_host: host, api_key: std::env::var("QWEATHER_API_KEY").unwrap_or_default(), jwt: std::env::var("QWEATHER_JWT").unwrap_or_default() }
        } else {
            let bytes = std::fs::read(config_path()?).map_err(|_| "和风天气尚未配置，请在宿主设置 API Host 和认证信息")?;
            serde_json::from_slice(&bytes).map_err(|_| "和风天气宿主配置格式错误")?
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        let host = self.api_host.as_str();
        if host.len() > 253 || !host.ends_with(".qweatherapi.com") ||
            host.split('.').any(|label| label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-') || !label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')) {
            return Err("和风 API Host 必须是控制台分配的 qweatherapi.com 域名，不包含协议或路径".into());
        }
        if self.api_key.is_empty() == self.jwt.is_empty() {
            return Err("和风天气宿主必须配置一种认证方式：API Key 或 JWT".into());
        }
        if [&self.api_key, &self.jwt].iter().any(|v| v.len() > 8192 || v.bytes().any(|c| c.is_ascii_control() || c == b' ')) {
            return Err("和风天气认证配置无效".into());
        }
        Ok(())
    }
}

fn query(args: &Value) -> Result<&str, String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("").trim();
    if query.is_empty() || query.chars().count() > 100 || query.chars().any(char::is_control) {
        return Err("请输入 1–100 个字符的城市、区县或景点名称".into());
    }
    Ok(query)
}

fn fetch(config: &Config, path: &str, query: &str) -> Result<Value, String> {
    if path != CITY && path != SCENIC { return Err("此和风接口未获授权".into()); }
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(20)).redirects(0).build();
    let mut request = agent.get(&format!("https://{}{}", config.api_host, path))
        .query("location", query).query("number", "20").query("lang", "zh")
        .set("Accept", "application/json");
    if path == SCENIC { request = request.query("type", "scenic"); }
    request = if config.jwt.is_empty() { request.set("X-QW-Api-Key", &config.api_key) }
        else { request.set("Authorization", &format!("Bearer {}", config.jwt)) };
    let response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        // Do not echo URLs, headers or server messages containing credentials.
        Err(ureq::Error::Transport(_)) => return Err("和风地区搜索暂时不可用，请稍后重试".into()),
    };
    let status = response.status();
    let mut bytes = Vec::new();
    response.into_reader().take(MAX_BODY + 1).read_to_end(&mut bytes)
        .map_err(|_| "无法读取和风地区搜索结果")?;
    if bytes.len() as u64 > MAX_BODY { return Err("和风地区搜索结果过大".into()); }
    decode(status, &bytes)
}

fn decode(status: u16, bytes: &[u8]) -> Result<Value, String> {
    let data: Value = serde_json::from_slice(bytes).map_err(|_| service_error(&status.to_string()))?;
    // Current GeoAPI uses problem+json HTTP 400 for an empty place lookup.
    if status == 400 && data["error"]["type"] == "https://dev.qweather.com/docs/resource/error-code/#no-such-location" {
        return Ok(json!({"code":"404"}));
    }
    if status != 200 {
        return Err(match data["error"]["type"].as_str().unwrap_or("").rsplit('#').next().unwrap_or("") {
            "no-credit" | "overdue" => "和风账户额度不足或有欠费，已停止本次搜索".into(),
            "invalid-host" => "和风 API Host 配置不正确".into(),
            _ => service_error(&status.to_string()),
        });
    }
    match data.get("code").and_then(Value::as_str) {
        Some("200" | "404") => Ok(data),
        Some(code) => Err(service_error(code)),
        None => Err("和风地区搜索缺少状态码".into()),
    }
}

fn service_error(code: &str) -> String {
    match code {
        "401" => "和风天气认证失败，请检查宿主凭据",
        "402" => "和风账户额度不足，已停止本次搜索",
        "403" => "和风请求被拒绝，请检查 API 权限、Host 和账户额度",
        "429" => "和风请求达到限制，请稍后再试",
        _ => "和风地区搜索暂时不可用，请稍后重试",
    }.into()
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok()).filter(|v| v.is_finite())
}

fn normalize(data: &Value, field: &str) -> Vec<Value> {
    if data["code"] != "200" { return Vec::new(); }
    let Some(rows) = data[field].as_array() else { return Vec::new() };
    rows.iter().filter_map(|row| {
        let name = row["name"].as_str()?.trim();
        let id = row["id"].as_str()?;
        let lat = number(&row["lat"])?;
        let lon = number(&row["lon"])?;
        if name.is_empty() || id.is_empty() || !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) { return None; }
        Some(json!({"name":name, "id":id, "admin1":row["adm1"].as_str().unwrap_or(""),
            "admin2":row["adm2"].as_str().unwrap_or(""), "country":row["country"].as_str().unwrap_or(""),
            "latitude":lat, "longitude":lon, "timezone":row["tz"].as_str().unwrap_or(""),
            "source":"QWeather", "type":row["type"].as_str().unwrap_or("city")}))
    }).take(20).collect()
}

struct Cached { at: Instant, value: Value }
static CACHE: OnceLock<Mutex<HashMap<(String, String, bool), Cached>>> = OnceLock::new();

pub fn search(app: &str, args: &Value) -> Result<Value, String> {
    if app != super::APP && app != "weather-assistant" { return Err("地区搜索仅供 Daycast 使用".into()); }
    let query = query(args)?;
    let config = Config::load()?;
    let scenic = args.get("scenic").and_then(Value::as_bool).unwrap_or(false);
    let key = (config.api_host.clone(), query.to_lowercase(), scenic);
    // Serializing duplicate searches prevents double billing while a first
    // request is in flight. At most two requests per uncached search; no retries.
    let mut cache = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().map_err(|_| "地区搜索繁忙，请重试")?;
    cache.retain(|_, entry| entry.at.elapsed() < CACHE_TTL);
    if let Some(entry) = cache.get(&key) { return Ok(entry.value.clone()); }
    let mut results = normalize(&fetch(&config, CITY, query)?, "location");
    // Only the regular search asks for scenery. AI family-city resolution
    // always uses cities, and doesn't select a scenic POI as a home city.
    let mut warning = "";
    if scenic {
        match fetch(&config, SCENIC, query) {
            Ok(data) => for row in normalize(&data, "poi") {
                if !results.iter().any(|r| r["id"] == row["id"]) { results.push(row); }
            },
            Err(_) if !results.is_empty() => { warning = "城市结果已找到，景点查询暂时不可用"; }
            Err(error) => return Err(error),
        }
    }
    let value = json!({"results":results,"source":"QWeather","warning":warning});
    // Do not cache an incomplete answer, so a later search can try scenery again.
    if warning.is_empty() {
        if cache.len() >= 256 { cache.clear(); }
        cache.insert(key, Cached { at:Instant::now(), value:value.clone() });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_rejects_unrelated_hosts_and_ambiguous_auth() {
        for host in ["evil.example", "api.qweatherapi.com.evil.example", "https://test.qweatherapi.com", "test.qweatherapi.com/path", "test.qweatherapi.com:443", "x@qweatherapi.com", ".qweatherapi.com"] {
            assert!(Config { api_host:host.into(), api_key:"test".into(), jwt:String::new() }.validate().is_err());
        }
        assert!(Config { api_host:"test.re.qweatherapi.com".into(), api_key:"test".into(), jwt:String::new() }.validate().is_ok());
        assert!(Config { api_host:"test.re.qweatherapi.com".into(), api_key:"test".into(), jwt:"token".into() }.validate().is_err());
    }

    #[test]
    fn places_keep_distinct_districts_and_use_valid_numeric_coordinates() {
        let data = json!({"code":"200","location":[
            {"id":"1","name":"朝阳","lat":"39.92","lon":"116.44","adm1":"北京","adm2":"北京"},
            {"id":"2","name":"朝阳","lat":"41.57","lon":"120.45","adm1":"辽宁","adm2":"朝阳"},
            {"id":"3","name":"无效","lat":"NaN","lon":"116"},
            {"id":"4","name":"越界","lat":"91","lon":"116"}
        ]});
        let places = normalize(&data, "location");
        assert_eq!(places.len(), 2);
        assert_eq!(places[0]["latitude"], 39.92);
        assert_eq!(places[1]["admin1"], "辽宁");
        assert!(normalize(&json!({"code":"404"}), "location").is_empty());
    }

    #[test]
    fn rejects_other_callers_before_loading_secrets_or_network() {
        assert!(search("os.news", &json!({"query":"北京"})).is_err());
        assert!(query(&json!({"query":"\n"})).is_err());
        assert!(query(&json!({"query":"x".repeat(101)})).is_err());
    }

    #[test]
    fn no_such_location_is_empty_but_other_400_responses_are_errors() {
        let empty = br#"{"error":{"type":"https://dev.qweather.com/docs/resource/error-code/#no-such-location"}}"#;
        assert_eq!(decode(400, empty).unwrap()["code"], "404");
        assert!(decode(400, br#"{"error":{"type":"https://dev.qweather.com/docs/resource/error-code/#invalid-parameters"}}"#).is_err());
        assert!(decode(401, b"{}").is_err());
        assert!(decode(429, b"{}").is_err());
    }

    #[test]
    #[ignore = "requires the host's QWeather configuration; up to four GeoAPI calls"]
    fn live_district_and_scenic_search() {
        let district = search(super::super::APP, &json!({"query":"榆次区","scenic":true})).unwrap();
        assert!(district["results"].as_array().unwrap().iter().any(|p| p["name"] == "榆次"));
        assert_eq!(district["warning"], "");
        let cached = search(super::super::APP, &json!({"query":"榆次区","scenic":true})).unwrap();
        assert_eq!(district, cached);
        let scenic = search(super::super::APP, &json!({"query":"五台山","scenic":true})).unwrap();
        assert!(scenic["results"].as_array().unwrap().iter().any(|p| p["name"] == "五台山风景名胜区" && p["type"] == "scenic"));
    }
}

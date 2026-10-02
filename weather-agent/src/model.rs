// 共享数据模型：城市表、Open-Meteo 预报形状、中文天气文案、日程种子、聊天记录。
//
// 时间戳沿用 Open-Meteo 的 ISO 字符串（`timezone=auto`），已经是所选城市的
// 本地墙钟时间（含 DST），所以标签直接从字符串切出来读，绝不做二次换算。
// 数组一律按索引对齐 `time`，容忍 null 和长度不齐。
use std::borrow::Cow;
use makepad_widgets::makepad_micro_serde::*;

// ============================================================ 城市

#[derive(Clone, Debug, PartialEq)]
pub struct City {
    pub name: Cow<'static, str>,
    pub en: Cow<'static, str>,
    pub lat: f64,
    pub lon: f64,
}

impl City {
    pub fn custom(name: String, region: String, lat: f64, lon: f64) -> Self {
        Self {
            name: Cow::Owned(name),
            en: Cow::Owned(region),
            lat,
            lon,
        }
    }
}

pub const CITIES: [City; 9] = [
    City { name: Cow::Borrowed("北京"), en: Cow::Borrowed("中国"), lat: 39.90, lon: 116.40 },
    City { name: Cow::Borrowed("上海"), en: Cow::Borrowed("中国"), lat: 31.23, lon: 121.47 },
    City { name: Cow::Borrowed("广州"), en: Cow::Borrowed("中国"), lat: 23.13, lon: 113.26 },
    City { name: Cow::Borrowed("深圳"), en: Cow::Borrowed("中国"), lat: 22.54, lon: 114.06 },
    City { name: Cow::Borrowed("杭州"), en: Cow::Borrowed("中国"), lat: 30.27, lon: 120.15 },
    City { name: Cow::Borrowed("成都"), en: Cow::Borrowed("中国"), lat: 30.57, lon: 104.07 },
    City { name: Cow::Borrowed("西安"), en: Cow::Borrowed("中国"), lat: 34.34, lon: 108.94 },
    City { name: Cow::Borrowed("青岛"), en: Cow::Borrowed("中国"), lat: 36.07, lon: 120.38 },
    City { name: Cow::Borrowed("天津"), en: Cow::Borrowed("中国"), lat: 39.13, lon: 117.20 },
];

#[derive(Default, DeJson)]
struct GeocodingResponse {
    results: Option<Vec<GeocodingResult>>,
}

#[derive(Default, DeJson)]
struct GeocodingResult {
    name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    country: Option<String>,
    admin1: Option<String>,
    admin2: Option<String>,
    admin3: Option<String>,
    admin4: Option<String>,
}

#[derive(Default, DeJson)]
struct IpLocation {
    success: Option<bool>,
    message: Option<String>,
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn geocoding_url(query: &str) -> String {
    format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=100&language=zh&format=json",
        encode_query(query.trim())
    )
}

pub fn parse_geocoding(text: &str) -> Result<Vec<City>, String> {
    let response = GeocodingResponse::deserialize_json_lenient(text).map_err(|e| e.msg)?;
    Ok(response
        .results
        .unwrap_or_default()
        .into_iter()
        .filter_map(|place| {
            let name = place.name?;
            let lat = place.latitude?;
            let lon = place.longitude?;
            // Prefer the most local administrative names first. GeoNames may
            // expose a district as the result itself, or as admin2/admin3.
            let mut parts = vec![place.admin4, place.admin3, place.admin2, place.admin1, place.country]
                .into_iter()
                .flatten()
                .filter(|part| !part.trim().is_empty() && part != &name)
                .collect::<Vec<_>>();
            parts.dedup();
            let region = parts.join(" · ");
            Some(City::custom(name, region, lat, lon))
        })
        .collect())
}

pub fn parse_ip_location(text: &str) -> Result<City, String> {
    let place = IpLocation::deserialize_json_lenient(text).map_err(|e| e.msg)?;
    if place.success == Some(false) {
        return Err(place.message.unwrap_or_else(|| "定位服务没有返回结果".into()));
    }
    let name = place.city.filter(|s| !s.is_empty()).ok_or("没有定位到城市")?;
    let lat = place.latitude.ok_or("定位服务没有返回纬度")?;
    let lon = place.longitude.ok_or("定位服务没有返回经度")?;
    let region = [place.region, place.country]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty() && part != &name)
        .collect::<Vec<_>>()
        .join(" · ");
    Ok(City::custom(name, region, lat, lon))
}

/// 预报地址：Open-Meteo，免 key。
///
/// 露点 / 气压 / 能见度 / UV / 阵风都放在 `hourly=` 里取，而不加进 `current=`：
/// `current=` 的变量表更窄，多一个不认的就会整单 400；`hourly=` 覆盖全量变量，
/// 且同一段序列正好用来画 24 小时曲线。当前值按 [`current_index`] 从序列里取。
/// 注意变量名是 `wind_gusts_10m`（不是 `wind_speed_10m_gust`），`visibility` 单位是米。
pub fn forecast_url(city: &City) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}\
         &current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,precipitation,\
         weather_code,cloud_cover,wind_speed_10m,wind_direction_10m\
         &hourly=temperature_2m,weather_code,precipitation_probability,precipitation,\
         dew_point_2m,pressure_msl,visibility,uv_index,wind_gusts_10m,wind_direction_10m\
         &daily=temperature_2m_max,temperature_2m_min,weather_code,precipitation_sum,sunrise,sunset\
         &timezone=auto&forecast_days=10",
        city.lat, city.lon
    )
}

/// 截到小时：`2026-09-30T16:15` → `2026-09-30T16`。
fn hour_key(s: &str) -> &str {
    s.split_once(':').map_or(s, |(a, _)| a)
}

/// `current.time` 是分钟粒度（`2026-09-30T16:15`），hourly 只有整点，所以没有
/// 精确匹配。取不晚于观测小时的那一格——露点、气压、UV 这类只有小时分辨率的
/// 变量都从这里读。两边都截到小时后按字符串比大小即可。
pub fn current_index(h: &Hourly, current_time: &str) -> usize {
    // 认不出时间格式就当没有观测，退回序列头。
    if !current_time.contains(':') || !current_time.contains('T') {
        return 0;
    }
    let key = hour_key(current_time);
    h.time.iter().rposition(|t| hour_key(t) <= key).unwrap_or(0)
}

// ============================================================ 预报

#[derive(Clone, Debug, Default, DeJson)]
pub struct Forecast {
    pub timezone: Option<String>,
    pub current: Option<Current>,
    pub hourly: Option<Hourly>,
    pub daily: Option<Daily>,
}

#[derive(Clone, Debug, Default, DeJson)]
pub struct Current {
    pub time: Option<String>,
    pub temperature_2m: Option<f64>,
    pub apparent_temperature: Option<f64>,
    pub relative_humidity_2m: Option<f64>,
    pub precipitation: Option<f64>,
    pub weather_code: Option<i32>,
    pub cloud_cover: Option<f64>,
    pub wind_speed_10m: Option<f64>,
    pub wind_direction_10m: Option<f64>,
    pub is_day: Option<i32>,
}

#[derive(Clone, Debug, Default, DeJson)]
pub struct Hourly {
    pub time: Vec<String>,
    pub temperature_2m: Vec<f64>,
    pub weather_code: Vec<i32>,
    pub precipitation_probability: Vec<f64>,
    pub precipitation: Vec<f64>,
    /// 露点，°C。
    pub dew_point_2m: Vec<f64>,
    /// 海平面气压，hPa。
    pub pressure_msl: Vec<f64>,
    /// 能见度，**米**（不是公里）。
    pub visibility: Vec<f64>,
    /// 紫外线指数，无量纲。
    pub uv_index: Vec<f64>,
    /// 阵风，km/h。
    pub wind_gusts_10m: Vec<f64>,
    pub wind_direction_10m: Vec<f64>,
}

#[derive(Clone, Debug, Default, DeJson)]
pub struct Daily {
    pub time: Vec<String>,
    pub temperature_2m_max: Vec<f64>,
    pub temperature_2m_min: Vec<f64>,
    pub weather_code: Vec<i32>,
    pub precipitation_sum: Vec<f64>,
    pub sunrise: Vec<String>,
    pub sunset: Vec<String>,
}

/// 宽松解析预报 JSON。Open-Meteo 的顶层还带 latitude / generationtime_ms 这类我们
/// 不收的字段，严格模式会在第一个不认识的就报 `Unexpected key latitude`。
pub fn parse_forecast(text: &str) -> Result<Forecast, String> {
    Forecast::deserialize_json_lenient(text).map_err(|e| e.msg)
}

// ============================================================ 文案

/// WMO 天气码 → 中文文案。只返回文字，字体里没有的符号一律不猜。
pub fn condition(code: i32) -> &'static str {
    match code {
        0 => "晴",
        1 => "大部晴朗",
        2 => "多云",
        3 => "阴",
        45 | 48 => "雾",
        51 | 53 | 55 => "毛毛雨",
        56 | 57 => "冻毛毛雨",
        61 | 63 | 65 => "雨",
        66 | 67 => "冻雨",
        71 | 73 | 75 => "雪",
        77 => "雪粒",
        80 | 81 | 82 => "阵雨",
        85 | 86 => "阵雪",
        95 => "雷阵雨",
        96 | 99 => "强雷阵雨",
        _ => "未知",
    }
}

pub fn weather_icon(code: i32) -> &'static str {
    match code {
        0 => "☀",
        1 | 2 => "⛅",
        3 | 45 | 48 => "☁",
        51..=67 | 80..=82 => "☂",
        71..=77 | 85..=86 => "❄",
        95..=99 => "⚡",
        _ => "◌",
    }
}

/// "2026-09-27T08:00" → "08:00"
pub fn hhmm(s: &str) -> &str {
    match s.find('T') {
        Some(p) => &s[p + 1..],
        None => s,
    }
}

/// "2026-09-27T08:00" → "2026-09-27"。没有 `T` 就原样返回。
pub fn date_of(s: &str) -> &str {
    match s.find('T') {
        Some(p) => &s[..p],
        None => s,
    }
}

/// "2026-09-27" → "周六"。用朱利安日数算，不引日期库。
pub fn weekday(s: &str) -> &'static str {
    let names: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
    let p: Vec<i64> = s.split('-').filter_map(|x| x.parse::<i64>().ok()).collect();
    if p.len() < 3 {
        return "—";
    }
    let (mut y, mut m) = (p[0], p[1]);
    let d = p[2];
    let a = (14 - m) / 12;
    y += 4800 - a;
    m += 12 * a - 3;
    let jdn = d + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045;
    let w = ((jdn % 7) + 7) % 7;
    names[(w as usize) % 7]
}

/// 按城市本地时区的今天日期，"YYYY-MM-DD"。
pub fn today_local() -> String {
    chrono_local_date()
}

/// 本地日期（无外部依赖，用系统时钟）。
fn chrono_local_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // 中国标准时间 UTC+8，小程序主要面向国内城市。
    let day = (secs + 8 * 3600) / 86400;
    let (y, m, d) = civil_from_days(day);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 从 1970-01-01 起的天数还原成公历。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ============================================================ 日程

#[derive(Clone, Debug)]
pub struct ScheduleItem {
    pub id: u32,
    /// "2026-09-27"
    pub date: String,
    /// "09:30"
    pub time: String,
    pub title: String,
    pub place: String,
    pub done: bool,
    /// 0=工作 1=生活 2=健康 3=其他
    pub kind: u8,
}

/// 第一版没有本地存储，用固定种子让页面不空。
pub fn seed_schedule() -> Vec<ScheduleItem> {
    let t = today_local();
    let m = next_local_date(1);
    let n = next_local_date(3);
    vec![
        ScheduleItem { id: 1, date: t.clone(), time: "09:30".into(), title: "周会：串联进度同步".into(), place: "会议室 B".into(), done: false, kind: 0 },
        ScheduleItem { id: 2, date: t.clone(), time: "12:30".into(), title: "午饭".into(), place: "".into(), done: false, kind: 1 },
        ScheduleItem { id: 3, date: t.clone(), time: "15:00".into(), title: "UI 框架评审".into(), place: "线上".into(), done: false, kind: 0 },
        ScheduleItem { id: 4, date: t.clone(), time: "19:30".into(), title: "跑步 5 公里".into(), place: "江边".into(), done: false, kind: 2 },
        ScheduleItem { id: 5, date: m, time: "10:00".into(), title: "深潜闪模型联调".into(), place: "".into(), done: false, kind: 0 },
        ScheduleItem { id: 6, date: n, time: "20:00".into(), title: "牙医复诊".into(), place: "口腔诊所".into(), done: false, kind: 2 },
    ]
}

/// 本地日期往后推几天，"YYYY-MM-DD"。
pub fn next_local_date(days: i64) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let day = (secs + 8 * 3600) / 86400 + days;
    let (y, m, d) = civil_from_days(day);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

// ============================================================ 聊天

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    User,
    Agent,
    Note,
}

#[derive(Clone, Debug)]
pub struct ScheduleProposal {
    pub title: String,
    pub date: String,
    pub time: String,
    pub place: String,
}

#[derive(Clone, Debug)]
pub struct ChatEntry {
    pub who: Who,
    pub title: String,
    pub text: String,
    pub choices: Vec<String>,
    pub schedule_proposal: Option<ScheduleProposal>,
    /// AppCard-style approval card: `None` is awaiting a decision, then Some
    /// records the user's durable choice in the conversation history.
    pub approval_decision: Option<bool>,
}

impl ChatEntry {
    pub fn new(who: Who, title: impl Into<String>, text: impl Into<String>) -> Self {
        Self { who, title: title.into(), text: text.into(), choices: Vec::new(), schedule_proposal: None, approval_decision: None }
    }

    pub fn approval(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self { who: Who::Agent, title: title.into(), text: text.into(), choices: Vec::new(), schedule_proposal: None, approval_decision: None }
    }

    pub fn is_approval(&self) -> bool {
        self.approval_decision.is_some() || self.title == "日程权限申请"
    }
}

// ============================================================ agent 工具的结果文案

/// 把当前预报压成给模型看的一段话。工具的返回值就是它。
pub fn ai_summary(city: &City, f: &Option<Forecast>, status_ok: bool) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(s, "城市：{}（{}），", city.name, city.en);
    let Some(f) = f else {
        if !status_ok {
            return format!("{}当前没有可用的预报数据，网络请求失败。", city.name);
        }
        return format!("{}当前正在获取预报数据，尚未拿到。", city.name);
    };
    let _ = write!(s, "数据来自 Open-Meteo。\n");
    if let Some(c) = &f.current {
        let code = c.weather_code.unwrap_or(-1);
        let _ = write!(
            s,
            "实况：{}，温度 {}℃，体感 {}℃，湿度 {}%，降水 {}mm，云量 {}%，风速 {}km/h，风力方位 {}°。\n",
            condition(code),
            c.temperature_2m.map(|v| v.round() as i32).unwrap_or(0),
            c.apparent_temperature.map(|v| v.round() as i32).unwrap_or(0),
            c.relative_humidity_2m.map(|v| v.round() as i32).unwrap_or(0),
            c.precipitation.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "0".into()),
            c.cloud_cover.map(|v| v.round() as i32).unwrap_or(0),
            c.wind_speed_10m.map(|v| v.round() as i32).unwrap_or(0),
            c.wind_direction_10m.map(|v| v.round() as i32).unwrap_or(0),
        );
    }
    if let Some(h) = &f.hourly {
        let n = h.time.len().min(24);
        let mut rain_hours = Vec::new();
        let mut max_t = f64::MIN;
        let mut min_t = f64::MAX;
        for i in 0..n {
            if let (Some(t), Some(p)) = (h.temperature_2m.get(i).copied(), h.precipitation_probability.get(i).copied()) {
                max_t = max_t.max(t);
                min_t = min_t.min(t);
                if p >= 50.0 {
                    rain_hours.push(hhmm(h.time[i].as_str()).to_string());
                }
            }
        }
        let _ = write!(
            s,
            "未来 24 小时：最高 {}℃，最低 {}℃，",
            max_t.round() as i32,
            min_t.round() as i32,
        );
        if rain_hours.is_empty() {
            s.push_str("无明显降水时段。\n");
        } else {
            let _ = write!(s, "降水概率≥50% 的小时：{}。\n", rain_hours.join("、"));
        }
    }
    if let Some(d) = &f.daily {
        let n = d.time.len().min(5);
        if n > 0 {
            s.push_str("未来 5 天：\n");
            for i in 0..n {
                let Some(day) = d.time.get(i) else { continue };
                let hi = d.temperature_2m_max.get(i).copied().unwrap_or(0.0).round() as i32;
                let lo = d.temperature_2m_min.get(i).copied().unwrap_or(0.0).round() as i32;
                let code = d.weather_code.get(i).copied().unwrap_or(-1);
                let rain = d.precipitation_sum.get(i).copied().unwrap_or(0.0);
                let _ = write!(
                    s,
                    "  {}（{}）：{}，{}~{}℃，累计降水 {:.1}mm\n",
                    date_of(day),
                    weekday(date_of(day)),
                    condition(code),
                    lo,
                    hi,
                    rain,
                );
            }
        }
        for i in 0..d.time.len().min(1) {
            let Some(sr) = d.sunrise.get(i) else { continue };
            let Some(ss) = d.sunset.get(i) else { continue };
            let _ = write!(s, "日出 {}，日落 {}。\n", hhmm(sr), hhmm(ss));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weekday_math_round_trips() {
        // 2000-01-01 是周六。
        assert_eq!(weekday("2000-01-01"), "周六");
        assert_eq!(weekday("2000-01-02"), "周日");
        assert_eq!(weekday("1999-12-31"), "周五");
        assert_eq!(weekday("2024-02-29"), "周四");
        assert_eq!(weekday("junk"), "—");
    }

    #[test]
    fn the_url_carries_the_city() {
        let u = forecast_url(&CITIES[0]);
        assert!(u.starts_with("https://api.open-meteo.com/v1/forecast?latitude=39.9000&longitude=116.4000&current="));
    }

    #[test]
    fn the_hourly_block_requests_the_atmosphere_variables() {
        // 露点 / 气压 / 能见度 / UV / 阵风都靠这段；`wind_gusts_10m` 是唯一正确的阵风变量名。
        let u = forecast_url(&CITIES[0]);
        // 抠出 `&hourly=...` 到下一个 `&` 之间，按逗号切成变量列表——
        // 整段里出现这个子串不足以证明它在该列表内。
        let list = |key: &str| -> Vec<String> {
            let from = u.find(key).unwrap();
            let start = from + key.len();
            let end = u[start..].find('&').map_or(u.len(), |i| start + i);
            u[start..end].split(',').map(str::to_string).collect()
        };
        let hourly = list("&hourly=");
        for v in ["dew_point_2m", "pressure_msl", "visibility", "uv_index", "wind_gusts_10m"] {
            assert!(hourly.contains(&v.to_string()), "{v}");
        }
        assert!(!u.contains("wind_speed_10m_gust"), "已弃用的阵风变量名会整单 400");
        // 这些不进 current=：那一栏变量表更窄，多一个不认的就整单失败。
        let current = list("&current=");
        assert!(!current.contains(&"dew_point_2m".to_string()));
        assert!(!current.contains(&"uv_index".to_string()));
    }

    #[test]
    fn current_index_tracks_the_observation_hour() {
        let h = Hourly {
            time: vec![
                "2026-09-30T14:00".into(),
                "2026-09-30T15:00".into(),
                "2026-09-30T16:00".into(),
                "2026-09-30T17:00".into(),
            ],
            ..Default::default()
        };
        // current.time 带分钟，没有整点精确匹配：取不晚于观测小时的那一格。
        assert_eq!(current_index(&h, "2026-09-30T16:15"), 2);
        assert_eq!(current_index(&h, "2026-09-30T16:00"), 2);
        assert_eq!(current_index(&h, "2026-09-30T15:59"), 1);
        // 早于序列头不越界，晚于序列尾取最后一格。
        assert_eq!(current_index(&h, "2026-09-30T00:00"), 0);
        assert_eq!(current_index(&h, "2026-09-30T23:00"), 3);
        assert_eq!(current_index(&h, "2026-09-29T09:00"), 0);
        assert_eq!(current_index(&h, "garbage"), 0);
    }

    #[test]
    fn the_summary_reads_as_prose() {
        let f = Some(Forecast {
            timezone: Some("Asia/Shanghai".into()),
            current: Some(Current {
                time: Some("2026-09-27T08:00".into()),
                temperature_2m: Some(26.4),
                apparent_temperature: Some(28.1),
                relative_humidity_2m: Some(72.0),
                precipitation: Some(0.0),
                weather_code: Some(61),
                cloud_cover: Some(60.0),
                wind_speed_10m: Some(9.0),
                wind_direction_10m: Some(135.0),
                is_day: Some(1),
            }),
            hourly: Some(Hourly {
                time: vec!["2026-09-27T08:00".into(), "2026-09-27T09:00".into()],
                temperature_2m: vec![26.0, 27.0],
                weather_code: vec![61, 3],
                precipitation_probability: vec![80.0, 65.0],
                precipitation: vec![1.4, 0.0],
                ..Default::default()
            }),
            daily: Some(Daily {
                time: vec!["2026-09-27".into()],
                temperature_2m_max: vec![29.0],
                temperature_2m_min: vec![22.0],
                weather_code: vec![61],
                precipitation_sum: vec![3.5],
                sunrise: vec!["05:52".into()],
                sunset: vec!["18:40".into()],
            }),
        });
        let s = ai_summary(&CITIES[0], &f, true);
        assert!(s.contains("北京"));
        assert!(s.contains("雨"));
        assert!(s.contains("降水概率"));
        assert!(s.contains("09:00"));
    }
}

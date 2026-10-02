// 天气页：实况 · 24 小时温度 · 五项指标 · 风况 · 十天预报。
//
// 视觉移植自 `weather-pages/v2-atmosphere.html`：fbm 流场天空 + 毛玻璃面板 +
// 角标刻度 + HUD 等宽字。数据来自 Open-Meteo（免 key）。`WeatherState` 由
// shell 持有，控制台里的 agent 调 `weather.current` 工具时读的是同一份，所以
// 状态不放在这一页里。
//
// 顶部观测地点只剩「城市名 + 切换」两件东西；点切换进 `PlacePickerScreen`
// （在 `place_picker.rs`），那里才有搜索框和搜索结果。`WeatherState` 上的
// `place_*` 字段留给 picker 用——picker 和天气页共用同一份状态。
//
// 页面上全是预声明槽位（`mc0..mc4` / `d0..d9`），运行时按索引刷：这个 makepad
// 里没有 `cx.builder`，动态子树只能提前摆好，不够用的藏起来。
use makepad_widgets::*;

use crate::model::{self, CITIES, Forecast};

/// 天空底图。原来是 fbm 流场 shader，但它的调色板本身偏暗，再过一遍
/// Reinhard + `pow(col, 0.88)` 就塌成一片蓝，索性换成烘焙好的图。
/// `SKY_BACKDROP_PATH` 是缓存键，带上应用前缀免得跟别的客户端的同名图撞车。
const SKY_BACKDROP: &[u8] = include_bytes!("assets/sky-backdrop.png");
const SKY_BACKDROP_PATH: &str = "weather-agent/sky-backdrop.png";

/// 响应体上限，别把窗格内存吃光。
const MAX_BODY_BYTES: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq)]
pub enum WxStatus {
    /// 还没发过请求。
    Idle,
    /// 有请求在飞。
    Loading,
    /// 最近一次取数成了。
    Done,
    /// 上一次失败了。
    Error(String),
}

/// 天气这一路的状态。`flight` 记下在途请求的 id 和它对应的城市；城市切走了
/// 旧响应就直接丢掉，不会把上一城的数画到新城头上。
#[derive(Clone)]
pub struct WeatherState {
    pub city: usize,
    pub custom_city: Option<model::City>,
    pub forecast: Option<Forecast>,
    pub status: WxStatus,
    pub flight: Option<(LiveId, usize)>,
    place_flight: Option<(LiveId, PlaceRequest)>,
    pub place_status: String,
    pub place_results: Vec<model::City>,
    pub located_city: Option<model::City>,
    /// 每次 `start` 自增。画 24 小时曲线时用它当缓存键的一部分。
    pub seq: u64,
    place_seq: u64,
}

#[derive(Clone, Copy)]
enum PlaceRequest {
    Search,
    Locate,
}

impl WeatherState {
    pub fn new() -> Self {
        Self {
            city: 0,
            custom_city: None,
            forecast: None,
            status: WxStatus::Idle,
            flight: None,
            place_flight: None,
            place_status: "支持搜索到区县，选中结果即可查看当地天气".into(),
            place_results: Vec::new(),
            located_city: None,
            seq: 0,
            place_seq: 0,
        }
    }

    pub fn city(&self) -> &model::City {
        self.custom_city.as_ref().unwrap_or(&CITIES[self.city.min(CITIES.len() - 1)])
    }

    pub fn has_data(&self) -> bool {
        self.forecast.is_some()
    }

    pub fn status_text(&self) -> String {
        match &self.status {
            WxStatus::Idle => "还没取数".into(),
            WxStatus::Loading => "正在取数…".into(),
            WxStatus::Done => "已更新".into(),
            WxStatus::Error(error) => format!("取数失败：{error}"),
        }
    }

    /// 换城市。返回 true 表示真的换了，该重新取数。
    pub fn select(&mut self, city: usize) -> bool {
        let city = city.min(CITIES.len() - 1);
        if self.custom_city.is_none() && city == self.city {
            return false;
        }
        self.city = city;
        self.custom_city = None;
        self.place_results.clear();
        self.place_status = "已切换城市".into();
        true
    }

    pub fn select_location(&mut self, city: model::City) {
        self.custom_city = Some(city);
        self.place_results.clear();
        self.place_status = "已选择地区".into();
    }

    fn begin_place_request(&mut self, cx: &mut Cx, kind: PlaceRequest, url: String) {
        if let Some((id, _)) = self.place_flight.take() {
            cx.cancel_http_request(id);
        }
        self.place_seq += 1;
        let id = LiveId::from_str(&format!("wx_place_{}", self.place_seq));
        self.place_flight = Some((id, kind));
        let mut request = HttpRequest::new(url, HttpMethod::GET);
        request.set_header("Accept".into(), "application/json".into());
        cx.http_request(id, request);
    }

    pub fn search_city(&mut self, cx: &mut Cx, query: &str) {
        let query = query.trim();
        self.place_results.clear();
        // 中文单字有意义（"天" → 天津 / 天水 / 天门 / ...）；1 字就开搜。
        if query.chars().count() < 1 {
            self.place_status = "输入地区再搜索".into();
            return;
        }
        self.place_status = format!("正在搜索“{query}”…");
        self.begin_place_request(cx, PlaceRequest::Search, model::geocoding_url(query));
    }

    pub fn locate_by_network(&mut self, cx: &mut Cx) {
        self.place_status = "正在按网络位置估算…".into();
        self.begin_place_request(cx, PlaceRequest::Locate, "https://ipwho.is/".into());
    }

    pub fn owns_place_request(&self, request_id: LiveId) -> bool {
        self.place_flight.is_some_and(|(id, _)| id == request_id)
    }

    pub fn resolve_place(&mut self, request_id: LiveId, result: Result<HttpResponse, String>) {
        let Some((_, kind)) = self.place_flight else { return };
        if !self.owns_place_request(request_id) {
            return;
        }
        self.place_flight = None;
        let parsed = result.and_then(|resp| {
            if resp.status_code != 200 {
                return Err(format!("HTTP {}", resp.status_code));
            }
            let body = resp.get_body().ok_or("没有响应体")?;
            if body.len() > MAX_BODY_BYTES {
                return Err("响应体过大".into());
            }
            let text = std::str::from_utf8(body).map_err(|_| "响应不是 UTF-8")?;
            match kind {
                PlaceRequest::Search => model::parse_geocoding(text),
                PlaceRequest::Locate => model::parse_ip_location(text).map(|city| vec![city]),
            }
        });
        match parsed {
            Ok(cities) if matches!(kind, PlaceRequest::Search) => {
                self.place_results = cities;
                self.place_status = if self.place_results.is_empty() {
                    "没有找到地区；可以输入“天津 东丽区”这样的城市+区县组合".into()
                } else {
                    format!("找到 {} 个结果，点选一个查看天气", self.place_results.len())
                };
            }
            Ok(mut cities) => {
                self.located_city = cities.pop();
                if let Some(city) = &self.located_city {
                    self.place_status = format!("定位到 {}，正在更新天气", city.name);
                }
            }
            Err(error) => {
                self.place_results.clear();
                self.place_status = format!("地区查询失败：{error}");
            }
        }
    }

    pub fn take_located_city(&mut self) -> Option<model::City> {
        self.located_city.take()
    }

    /// 发一次请求。旧的在途请求先取消。
    pub fn start(&mut self, cx: &mut Cx) {
        if let Some((id, _)) = self.flight.take() {
            cx.cancel_http_request(id);
        }
        self.seq += 1;
        let id = LiveId::from_str(&format!("wx_forecast_{}", self.seq));
        self.flight = Some((id, self.city));
        self.status = WxStatus::Loading;
        let mut request = HttpRequest::new(model::forecast_url(self.city()), HttpMethod::GET);
        request.set_header("Accept".into(), "application/json".into());
        cx.http_request(id, request);
    }

    pub fn owns(&self, request_id: LiveId) -> bool {
        self.flight.is_some_and(|(id, _)| id == request_id)
    }

    /// 消化一个网络响应。不是本状态的请求就直接丢掉。
    pub fn resolve(&mut self, request_id: LiveId, result: Result<HttpResponse, String>) {
        if !self.owns(request_id) {
            return;
        }
        self.flight = None;
        let parsed = result.and_then(|resp| {
            if resp.status_code != 200 {
                return Err(format!("HTTP {}", resp.status_code));
            }
            let Some(body) = resp.get_body() else {
                return Err("没有响应体".into());
            };
            if body.len() > MAX_BODY_BYTES {
                return Err("响应体过大".into());
            }
            let Ok(text) = std::str::from_utf8(body) else {
                return Err("响应不是 UTF-8".into());
            };
            // 宽松解析，见 model::parse_forecast 的注释。
            model::parse_forecast(text)
        });
        match parsed {
            Ok(f) => {
                self.forecast = Some(f);
                self.status = WxStatus::Done;
            }
            Err(e) => self.status = WxStatus::Error(e),
        }
    }
}

// 槽位名。`LiveId::from_str` 和 `ids!` 是同一个哈希，所以能按索引取槽位。
const RAIL_SLOTS: [&str; 5] = ["mc0", "mc1", "mc2", "mc3", "mc4"];
const DAY_SLOTS: [&str; 10] = ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9"];

fn slot(names: &[&str], i: usize) -> LiveId {
    LiveId::from_str(names[i.min(names.len() - 1)])
}

/// "2026-09-28" → "9月28日 周一"
fn date_zh(s: &str) -> String {
    let p: Vec<&str> = s.split('-').collect();
    if p.len() < 3 {
        return s.to_string();
    }
    let (m, d) = (p[1], p[2]);
    format!(
        "{}月{}日 {}",
        m.trim_start_matches('0'),
        d.trim_start_matches('0'),
        model::weekday(s)
    )
}

/// 第一行「今天」、第二行「明天」，其余给星期几。
fn day_name(date: &str) -> String {
    if date == model::today_local() {
        "今天".into()
    } else if date == model::next_local_date(1) {
        "明天".into()
    } else {
        model::weekday(date).to_string()
    }
}

/// 状态行的文字。`Done` 不会走到这里——`Done` 意味着上一次取成了，`forecast` 一定在。
fn status_line(s: &WxStatus) -> String {
    match s {
        WxStatus::Idle => "暂无数据".into(),
        WxStatus::Loading => "加载中…".into(),
        WxStatus::Done => "暂无数据".into(),
        WxStatus::Error(error) => format!("取数失败：{error}"),
    }
}

/// 没有预报数据时，实况标题位显示什么。不出现"还没取数"这类传输层措辞。
fn empty_condition(s: &WxStatus) -> &'static str {
    match s {
        WxStatus::Loading => "加载中…",
        WxStatus::Error(_) => "连接暂不可用",
        _ => "暂无数据",
    }
}

/// Screen copy keeps transport details out of the weather page; the assistant
/// and diagnostic state still retain the original error text.
fn screen_status(s: &WxStatus) -> String {
    if matches!(s, WxStatus::Error(_)) {
        "连接暂不可用 · 点右上角重试".into()
    } else {
        status_line(s)
    }
}

/// 方位角（风的来向）→ 中文八方位。
fn compass_zh(dir: f64) -> &'static str {
    let d = (dir.rem_euclid(360.0) + 22.5).rem_euclid(360.0);
    match (d / 45.0) as usize {
        0 => "北",
        1 => "东北",
        2 => "东",
        3 => "东南",
        4 => "南",
        5 => "西南",
        6 => "西",
        _ => "西北",
    }
}

/// 指标值：带上单位；取不到就显示占位。
fn fmt(v: Option<f64>, unit: &str) -> String {
    match v {
        Some(x) => format!("{:.0}{}", x.round(), unit),
        None => "—".into(),
    }
}

/// 需要精度的读数：一位小数。
fn fmt1(v: Option<f64>, unit: &str) -> String {
    match v {
        Some(x) => format!("{x:.1}{unit}"),
        None => "—".into(),
    }
}

/// 带符号的读数：气压趋势这类有正负的方向感。
fn fmt_signed(v: Option<f64>, unit: &str) -> String {
    match v {
        Some(x) => format!("{x:+.1}{unit}"),
        None => "—".into(),
    }
}

/// 降水量：零显示"无"，其余保留一位小数。
fn fmt_mm(v: Option<f64>) -> String {
    match v {
        Some(x) if x > 0.0 => format!("{x:.1}mm"),
        _ => "无".into(),
    }
}

/// UV 指数的等级词（WHO 分级）。
fn uv_word(v: f64) -> &'static str {
    if v < 3.0 {
        "低"
    } else if v < 6.0 {
        "中等"
    } else if v < 8.0 {
        "高"
    } else if v < 11.0 {
        "很高"
    } else {
        "极高"
    }
}

/// 24 小时趋势的方向箭头。
fn trend_arrow(delta: f64) -> &'static str {
    if delta > 0.05 {
        "↑"
    } else if delta < -0.05 {
        "↓"
    } else {
        "→"
    }
}

/// Makepad's animated WeatherIcon uses these condition buckets.
fn weather_art_kind(code: i32) -> f64 {
    match code {
        -1 => 2.0,                        // waiting for the first forecast
        0 => 0.0,                         // clear
        1 | 2 => 1.0,                     // mostly clear / partly cloudy
        3 => 2.0,                         // overcast
        45 | 48 => 7.0,                   // fog
        51..=67 | 80..=82 => 3.0,         // drizzle / rain
        71..=77 | 85..=86 => 5.0,         // snow
        95..=99 => 4.0,                   // thunder
        _ => 6.0,                         // windy / unsettled
    }
}

/// 背景色板随天气码换：晴偏暖、雨偏蓝绿、雾偏灰、雷暴偏紫。
///
/// 现在喂给 fbm 天空：`warm` 是暖度（0 冷 1 暖），`lit` 是明度。
fn weather_mood(code: i32) -> (f64, f64) {
    match code {
        0 => (0.66, 1.0),
        1 | 2 => (0.52, 0.98),
        3 => (0.30, 0.80),
        45 | 48 => (0.36, 0.62),
        51..=67 | 80..=82 => (0.20, 0.86),
        71..=77 | 85..=86 => (0.34, 0.90),
        95..=99 => (0.42, 0.94),
        _ => (0.32, 0.86),
    }
}

/// 天气主卡的天空色：晴天偏蓝，黄昏加暖色，夜间用沉静的蓝紫；降水按类型转冷。
fn hero_sky(code: i32, is_day: bool, hour: u32) -> (Vec4f, Vec4f) {
    if !is_day {
        return if (95..=99).contains(&code) {
            (vec4(0.16, 0.19, 0.34, 1.0), vec4(0.27, 0.20, 0.37, 1.0))
        } else if (51..=67).contains(&code) || (80..=82).contains(&code) {
            (vec4(0.10, 0.25, 0.39, 1.0), vec4(0.16, 0.37, 0.48, 1.0))
        } else {
            (vec4(0.12, 0.24, 0.42, 1.0), vec4(0.27, 0.28, 0.49, 1.0))
        };
    }
    if (16..=19).contains(&hour) {
        return (vec4(0.85, 0.43, 0.31, 1.0), vec4(0.48, 0.35, 0.62, 1.0));
    }
    match code {
        0 => (vec4(0.12, 0.54, 0.86, 1.0), vec4(0.39, 0.75, 0.91, 1.0)),
        1 | 2 => (vec4(0.19, 0.50, 0.77, 1.0), vec4(0.47, 0.70, 0.82, 1.0)),
        3 | 45 | 48 => (vec4(0.31, 0.41, 0.53, 1.0), vec4(0.51, 0.59, 0.65, 1.0)),
        51..=67 | 80..=82 => (vec4(0.14, 0.34, 0.51, 1.0), vec4(0.27, 0.51, 0.64, 1.0)),
        71..=77 | 85..=86 => (vec4(0.40, 0.58, 0.72, 1.0), vec4(0.68, 0.78, 0.84, 1.0)),
        95..=99 => (vec4(0.27, 0.29, 0.49, 1.0), vec4(0.45, 0.35, 0.59, 1.0)),
        _ => (vec4(0.24, 0.46, 0.62, 1.0), vec4(0.43, 0.62, 0.70, 1.0)),
    }
}

fn observation_hour(time: Option<&str>) -> u32 {
    time.and_then(|value| value.split('T').nth(1))
        .and_then(|clock| clock.get(..2))
        .and_then(|hour| hour.parse().ok())
        .unwrap_or(12)
}

/// 天气文案的强调色。
fn condition_ink(code: i32) -> Vec4f {
    match code {
        0 => vec4(0.98, 0.76, 0.40, 1.0),
        1 | 2 => vec4(0.62, 0.82, 0.92, 1.0),
        3 => vec4(0.72, 0.79, 0.85, 1.0),
        45 | 48 => vec4(0.68, 0.80, 0.84, 1.0),
        51..=67 | 80..=82 => vec4(0.42, 0.80, 0.96, 1.0),
        71..=77 | 85..=86 => vec4(0.72, 0.90, 0.94, 1.0),
        95..=99 => vec4(0.84, 0.70, 0.98, 1.0),
        _ => vec4(0.60, 0.90, 0.76, 1.0),
    }
}

/// 取当前观测时刻在 hourly 序列里的下标（露点、气压、UV、阵风都只有小时分辨率）。
fn cur_index(f: &Forecast) -> usize {
    match (&f.hourly, f.current.as_ref().and_then(|c| c.time.clone())) {
        (Some(h), Some(t)) => model::current_index(h, &t),
        _ => 0,
    }
}

/// 从一条可选序列里取第 i 个值。
fn pick(series: &Option<Vec<f64>>, i: usize) -> Option<f64> {
    series.as_ref().and_then(|s| s.get(i)).copied()
}

// —— 十天的笔画图标：`DrawSvg` 支持 `stroke-linecap`、`stroke-linejoin`、渐变和虚线，
// 所以直接内联一段 SVG，不需要外部资源文件。`{stroke}` 由调用方填颜色。
const GLYPH_SUN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><circle cx="13" cy="13" r="4.2" fill="none" stroke="{stroke}" stroke-width="1.4"/><path d="M13 2.6v2.2M13 21.2v2.2M2.6 13h2.2M21.2 13h2.2M5.4 5.4l1.6 1.6M19 19l1.6 1.6M20.6 5.4l-1.6 1.6M7 19l-1.6 1.6" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linecap="round"/></svg>"#;
const GLYPH_CLOUD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><path d="M7.2 19.5h11a3.7 3.7 0 0 0 0.5-7.3 5.2 5.2 0 0 0-9.8-1.5A4.4 4.4 0 0 0 7.2 19.5z" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linejoin="round"/></svg>"#;
const GLYPH_PARTLY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><circle cx="9.4" cy="9.2" r="3.1" fill="none" stroke="{stroke}" stroke-width="1.4"/><path d="M9.4 3.6v1.6M3.8 9.2H2.2M5 4.8l1.1 1.1M13.8 4.8l-1.1 1.1" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linecap="round"/><path d="M10.4 21h8.6a3.2 3.2 0 0 0 0.4-6.4 4.5 4.5 0 0 0-8.6-1.2A3.8 3.8 0 0 0 10.4 21z" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linejoin="round"/></svg>"#;
const GLYPH_FOG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><path d="M4.6 9h14M3.4 13h17M5 17h13M7.4 21h10" fill="none" stroke="{stroke}" stroke-width="1.5" stroke-linecap="round"/></svg>"#;
const GLYPH_RAIN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><path d="M6.8 16.5h11.6a3.4 3.4 0 0 0 0.4-6.8 4.9 4.9 0 0 0-9.3-1.3A4.1 4.1 0 0 0 6.8 16.5z" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linejoin="round"/><path d="M9.2 19.4l-1 2.6M13.6 19.4l-1 2.6M18 19.4l-1 2.6" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linecap="round"/></svg>"#;
const GLYPH_SNOW: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><path d="M6.8 15.5h11.6a3.4 3.4 0 0 0 0.4-6.8 4.9 4.9 0 0 0-9.3-1.3A4.1 4.1 0 0 0 6.8 15.5z" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linejoin="round"/><path d="M8.6 18.9v3M7.4 20.4h2.4M13.5 18.9v3M12.3 20.4h2.4M18.4 18.9v3M17.2 20.4h2.4" fill="none" stroke="{stroke}" stroke-width="1.2" stroke-linecap="round"/></svg>"#;
const GLYPH_THUNDER: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 26"><path d="M6.8 15.5h11.6a3.4 3.4 0 0 0 0.4-6.8 4.9 4.9 0 0 0-9.3-1.3A4.1 4.1 0 0 0 6.8 15.5z" fill="none" stroke="{stroke}" stroke-width="1.4" stroke-linejoin="round"/><path d="M13.9 15.4l-3 4.6h2.6l-1.4 3.4 4-5.2h-2.6z" fill="none" stroke="{stroke}" stroke-width="1.3" stroke-linejoin="round"/></svg>"#;

/// 天气码 → 一段可渲染的 SVG。`DrawSvg` 用 `meet` 铺 viewBox，所以这里只画
/// 26×26 的方框，尺寸交给 `svg_walk`。
fn glyph_svg(code: i32, stroke: &str) -> String {
    let t = match code {
        0 => GLYPH_SUN,
        1 | 2 => GLYPH_PARTLY,
        3 => GLYPH_CLOUD,
        45 | 48 => GLYPH_FOG,
        51..=67 | 80..=82 => GLYPH_RAIN,
        71..=77 | 85..=86 => GLYPH_SNOW,
        95..=99 => GLYPH_THUNDER,
        _ => GLYPH_CLOUD,
    };
    t.replace("{stroke}", stroke)
}

/// 24 小时温度曲线：Catmull-Rom 转三次贝塞尔，渐变填色 + 渐变描边 + 当前时刻标记。
///
/// `DrawSvg` 的 `viewbox_transform` 永远用 `meet`（等比缩放、居中留边），没有
/// `preserveAspectRatio="none"`。所以 viewBox 必须按控件的实测尺寸构造，
/// 否则整条曲线会被缩掉一截。
fn spark_svg(temps: &[f64], w: u32, h: u32, now: usize) -> String {
    if temps.len() < 2 || w < 40 || h < 60 {
        return String::new();
    }
    let w = w as f32;
    let h = h as f32;
    let pad_l = 32.0f32;
    let pad_r = 14.0f32;
    let pad_t = 22.0f32;
    let pad_b = 24.0f32;
    let iw = w - pad_l - pad_r;
    let ih = h - pad_t - pad_b;
    let (lo, hi) = temps.iter().fold((f64::MAX, f64::MIN), |(a, b), t| {
        (a.min(*t), b.max(*t))
    });
    let span = (hi - lo).max(4.0) as f32;
    let n = temps.len() as f32;
    let pts: Vec<(f32, f32)> = temps
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let y = ((1.0 - (*t - lo) / span as f64) as f32).clamp(0.0, 1.0);
            (pad_l + iw * i as f32 / (n - 1.0), pad_t + ih * y)
        })
        .collect();

    // Catmull-Rom 控制点：把相邻四段的中点连成光滑曲线。
    let mut line = format!("M {:.1} {:.1}", pts[0].0, pts[0].1);
    for i in 0..pts.len() - 1 {
        let a = if i == 0 { pts[0] } else { pts[i - 1] };
        let e = pts[(i + 2).min(pts.len() - 1)];
        let c1x = pts[i].0 + (pts[i + 1].0 - a.0) / 6.0;
        let c1y = pts[i].1 + (pts[i + 1].1 - a.1) / 6.0;
        let c2x = pts[i + 1].0 - (e.0 - pts[i].0) / 6.0;
        let c2y = pts[i + 1].1 - (e.1 - pts[i].1) / 6.0;
        line.push_str(&format!(
            " C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
            c1x,
            c1y,
            c2x,
            c2y,
            pts[i + 1].0,
            pts[i + 1].1
        ));
    }
    let base_y = h - pad_b;
    let area = format!(
        "{line} L {:.1} {:.1} L {:.1} {:.1} Z",
        pts.last().unwrap().0,
        base_y,
        pad_l,
        base_y
    );

    let mut grid = String::new();
    for (frac, val) in [(0.0f64, hi), (0.5, (lo + hi) / 2.0), (1.0, lo)] {
        let y = pad_t + ih * frac as f32;
        grid.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"rgba(132,196,255,0.10)\" stroke-width=\"1\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"rgba(157,180,212,0.85)\" font-family=\"monospace\" font-size=\"8.5\">{:.0}°</text>",
            pad_l, y, pad_l + iw, y, pad_l - 7.0, y + 3.0, val
        ));
    }
    let mut xs = String::new();
    for hr in [0usize, 4, 8, 12, 16, 20] {
        if hr as f32 >= n {
            continue;
        }
        let x = pad_l + iw * hr as f32 / (n - 1.0);
        xs.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"rgba(99,121,155,0.9)\" font-family=\"monospace\" font-size=\"8\" text-anchor=\"middle\">{:02}</text>",
            x,
            h - 7.0,
            hr
        ));
    }
    let now = now.min(pts.len() - 1);
    let (nx, ny) = pts[now];
    let nowmark = format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"rgba(79,216,255,0.40)\" stroke-width=\"1\" stroke-dasharray=\"3 3\"/><circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"5.5\" fill=\"rgba(79,216,255,0.16)\"/><circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.6\" fill=\"#04060F\" stroke=\"#4FD8FF\" stroke-width=\"1.6\"/>",
        nx, pad_t, nx, base_y, nx, ny, nx, ny
    );

    format!(
        r###"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0} {h:.0}">
<defs>
<linearGradient id="ar" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4FD8FF" stop-opacity="0.34"/><stop offset="0.55" stop-color="#2F7BF0" stop-opacity="0.13"/><stop offset="1" stop-color="#2F7BF0" stop-opacity="0"/></linearGradient>
<linearGradient id="ln" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#2F7BF0"/><stop offset="0.6" stop-color="#4FD8FF"/><stop offset="1" stop-color="#C862E8"/></linearGradient>
</defs>
{grid}
{xs}
<path d="{area}" fill="url(#ar)" stroke="none"/>
{nowmark}
<path d="{line}" fill="none" stroke="url(#ln)" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"###,
        w = w,
        h = h,
        grid = grid,
        xs = xs,
        area = area,
        nowmark = nowmark,
        line = line
    )
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.shader.*
    use mod.widgets.*

    // —— HUD 字体尺度 ——
    let Eyebrow = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #8FE1FF text_style: theme.font_code{font_size: 10.0 letter_spacing: 3.4} }
    }
    let CardH = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #8FE1FF text_style: theme.font_code{font_size: 8.0 letter_spacing: 2.0} }
    }
    let Tiny = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #63799B text_style: theme.font_code{font_size: 8.0 letter_spacing: 1.2} }
    }
    let TinyR = Label{
        padding: 0 margin: 0 width: Fill height: Fit max_lines: 1
        // 全角标点（U+FF0C ，/ U+FF1A ：）在 `liberation_mono` 里没有字形，
        // 中文正文走无衬线；等宽字只留给纯拉丁的 HUD 标签。
        draw_text +: { color: #63799B text_style: theme.font_regular{font_size: 9.0} }
    }
    let CnTiny = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #7C94B4 text_style: theme.font_regular{font_size: 9.0} }
    }
    let Ink2 = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #9DB4D4 text_style: theme.font_regular{font_size: 11.0} }
    }
    let CondZh = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #EAF4FF text_style: theme.font_bold{font_size: 22.0 letter_spacing: 4.0} }
    }
    let HeroNum = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #FFFFFF text_style: theme.font_regular{font_size: 66.0} }
    }
    let HeroDeg = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #F4A34B text_style: theme.font_bold{font_size: 26.0} }
    }
    let Lede = Label{
        padding: 0 margin: 0 width: Fill height: Fit max_lines: 2
        draw_text +: { color: #9DB4D4 text_style: theme.font_regular{font_size: 11.5 line_spacing: 1.9} }
    }
    let MetaKey = Label{
        padding: 0 margin: 0 width: Fill height: Fit align: Align{x: 0.5 y: 0.5}
        draw_text +: { color: #63799B text_style: theme.font_code{font_size: 8.0 letter_spacing: 1.8} }
    }
    let MetaVal = Label{
        padding: 0 margin: 0 width: Fill height: Fit align: Align{x: 0.5 y: 0.5}
        draw_text +: { color: #EAF4FF text_style: theme.font_bold{font_size: 15.0} }
    }
    let ChipName = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #B9C8D7 text_style: theme.font_regular{font_size: 11.0} }
    }

    // —— 运行时拼的 SVG（24 小时曲线、十天的笔画图标）——
    //
    // `Icon` 的 `draw_icon` 是私有字段，crate 外灌不进去，所以自带一个只包一层
    // `DrawSvg` 的壳。`mod.widgets.*` 的解析是顺序敏感的：这里必须在 DayRow 之前
    // 定义，DayRow 里才引用得到。
    mod.widgets.WxGlyphBase = #(WxGlyph::register_widget(vm))
    mod.widgets.WxGlyph = set_type_default() do mod.widgets.WxGlyphBase{
        width: Fit
        height: Fit
        svg_walk: Walk{ width: Fit height: Fit }
    }

    // —— 四角刻度：面板的九个直角小钩 ——
    mod.widgets.Ticks = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            pixel: fn(){
                let res = self.rect_size
                let p = self.pos * res
                let s = 10.0
                let t = 1.6
                let tlh = step(p.x, s) * (1.0 - step(t, p.y))
                let tlv = (1.0 - step(t, p.x)) * step(p.y, s)
                let trh = step(res.x - s, p.x) * (1.0 - step(t, p.y))
                let trv = (1.0 - step(res.x - s, p.x)) * step(res.x - t - s, p.x) * step(p.y, s)
                let blh = step(p.x, s) * step(res.y - s, p.y) * (1.0 - step(res.y - t, p.y))
                let blv = (1.0 - step(t, p.x)) * step(res.y - t - s, p.y) * (1.0 - step(res.y - s, p.y))
                let brh = step(res.x - s, p.x) * step(res.y - s, p.y) * (1.0 - step(res.y - t, p.y))
                let brv = (1.0 - step(res.x - s, p.x)) * step(res.x - t - s, p.x) * step(res.y - t - s, p.y) * (1.0 - step(res.y - s, p.y))
                let m = clamp(tlh + tlv + trh + trv + blh + blv + brh + brv, 0.0, 1.0)
                return vec4(0.32 * m, 0.85 * m, 1.0 * m, m * 0.62)
            }
        }
    }

    // —— 温度数字的辉光：高斯衰减叠加层 ——
    mod.widgets.Glow = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            amp: uniform(0.5)
            pixel: fn(){
                let q = self.pos - vec2(0.5, 0.5)
                let d = length(q)
                let f = exp(-d * d * 5.0) * self.amp
                return vec4(0.25 * f, 0.68 * f, 1.0 * f, f * 0.62)
            }
        }
    }

    // —— 顶栏的品牌圆点 ——
    mod.widgets.Orb = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            pixel: fn(){
                let q = self.pos * 2.0 - 1.0
                let d = length(q)
                let disk = 1.0 - smoothstep(0.86, 1.0, d)
                let halo = (1.0 - smoothstep(0.92, 1.8, d)) * 0.30
                let a = clamp(1.0 - d * 0.70, 0.0, 1.0)
                // shader 里 `let` 绑定不可重新赋值，要用 `var`。
                var col = mix(vec3(0.030, 0.110, 0.280), vec3(0.100, 0.520, 0.960), a)
                let g = exp(-pow(d - 0.52, 2.0) * 9.0)
                col = mix(col, vec3(0.860, 0.960, 1.000), g * 0.55)
                let al = disk + halo
                return vec4(col.x * al, col.y * al, col.z * al, disk + halo * 0.4)
            }
        }
    }

    // —— LIVE 心跳点 ——
    mod.widgets.LiveDot = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            pixel: fn(){
                let t = self.draw_pass.time
                let q = self.pos * 2.0 - 1.0
                let d = length(q)
                let p = fract(t * 0.48)
                let ring = (1.0 - smoothstep(0.0, 0.06, abs(d - p * 0.95))) * (1.0 - p)
                let core = 1.0 - smoothstep(0.30, 0.42, d)
                let glow = (1.0 - smoothstep(0.35, 1.0, d)) * (0.30 + 0.30 * sin(t * 3.1))
                let a = max(core, max(ring * 0.85, glow))
                return vec4(0.35 * a, 0.95 * a, 0.86 * a, a)
            }
        }
    }

    // —— 进度条：轨道 + 渐变填充 ——
    mod.widgets.Bar = View{
        width: Fill height: 3
        show_bg: true
        draw_bg +: {
            frac: uniform(0.0)
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.5)
                let t = self.pos.x
                let fill = step(t, self.frac)
                let c = mix(vec3(0.18, 0.48, 0.94), vec3(0.31, 0.85, 1.0), t)
                sdf.fill_keep(mix(vec4(0.07, 0.12, 0.21, 1.0), vec4(c.x, c.y, c.z, 1.0), fill))
                return sdf.result
            }
        }
    }

    // —— 天空底图 ——
    //
    // 之前这里是 fbm 流场 shader，删掉了；原因写在 `SKY_BACKDROP` 的注释上。
    // `Image` 直接内联在 `WeatherScreen` 的 DSL 里，见下面 `atmo := Image{...}`。

    // —— 毛玻璃面板 ——
    //
    // `GaussRoundedView` 自己开 overlay 抓底图做高斯模糊；没有快照时退化成
    // `fallback_color` 的实心面板，不会白屏。`blur_level` 是金字塔层数（0..6），
    // 不是像素——CSS 的 `blur(20px)` 大约等于 3.0~4.0。
    let GlassCard = GaussRoundedView{
        width: Fill height: Fit
        flow: Overlay
        clip_x: false
        clip_y: false
        draw_bg +: {
            corner_radius: 20.0
            blur_level: 3.4
            edge_blur_level: 1.0
            lensing_effect: 1.0
            lensing_strength: 1.6
            lensing_width: 7.0
            tint_color: #0B1526
            tint_alpha: 0.30
            surface_alpha: 1.0
            border_alpha: 0.20
            border_width: 0.5
            rim_alpha: 0.26
            rim_width: 1.25
            inner_shadow_alpha: 0.14
            inner_shadow_band: 2.5
            specular_strength: 0.0
            noise_strength: 0.002
            fallback_color: #0A1424
            shadow_radius: 0.0
        }
    }
    // 没有内容的实心卡片（地点搜索栏、十天列表这类不需要毛玻璃的地方）。
    let Panel = RoundedShadowView{
        width: Fill height: Fit
        flow: Down spacing: 12
        padding: Inset{top: 16 left: 18 right: 18 bottom: 16}
        show_bg: true
        draw_bg +: {
            color: #0C1728
            border_radius: 18.0
            border_size: 1.0
            border_color: #22384C
            shadow_color: #00000030
            shadow_radius: 14.0
        }
    }

    // —— 切换按钮 ——
    //
    // 「切换」点下去之后由 Shell 把 weather 页藏起来、亮起 PlacePickerScreen。
    // 同一颗按钮在取数中（`Loading`）转圈样式，所以 glyph 字段保留名字。
    let SwitchBtn = RoundedShadowView{
        width: Fit height: 36
        padding: Inset{top: 0 left: 14 right: 14 bottom: 0}
        margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #38302A border_radius: 12.0 border_size: 1.0 border_color: #534335 }
        glyph := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            text: "切换"
            draw_text +: { color: #F4A34B text_style: theme.font_bold{font_size: 11.0} }
        }
    }

    // 首页的轻量场景入口：场景问题交给宿主 AI，天气页只负责给出入口。
    let SceneChip = View{
        width: Fill height: 36
        padding: Inset{top: 0 left: 8 right: 8 bottom: 0}
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #403A32 border_radius: 18.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            draw_text +: { color: #E8DAC5 text_style: theme.font_regular{font_size: 10.5} }
        }
    }

    let HeroCard = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 8
        padding: Inset{top: 16 left: 17 right: 17 bottom: 15}
        show_bg: true
        draw_bg +: {
            sky_top: uniform(vec4(0.10, 0.48, 0.80, 1.0))
            sky_bottom: uniform(vec4(0.27, 0.68, 0.88, 1.0))
            sunlight: uniform(0.42)
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 20.0)
                let p = self.pos
                let base = mix(self.sky_top, self.sky_bottom, smoothstep(0.0, 1.0, p.y))
                sdf.fill_keep(base)
                let sun = exp(-length((p - vec2(0.86, 0.18)) * vec2(1.0, 1.4)) * 8.0) * self.sunlight
                sdf.fill_keep(vec4(1.0 * sun, 0.76 * sun, 0.42 * sun, sun * 0.30))
                return sdf.result
            }
        }
    }

    let DecisionCard = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 8
        padding: Inset{top: 14 left: 16 right: 16 bottom: 14}
        show_bg: true
        draw_bg +: { color: #252525 border_radius: 14.0 border_size: 0.8 border_color: #383838 }
    }

    let SmartCard = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 8
        padding: Inset{top: 14 left: 16 right: 16 bottom: 14}
        show_bg: true
        draw_bg +: { color: #30291F border_radius: 14.0 border_size: 0.8 border_color: #6C5438 }
    }

    // —— 五项指标卡 ——
    let RailCard = GaussRoundedView{
        width: Fill height: Fit
        flow: Overlay
        clip_x: false
        clip_y: false
        draw_bg +: {
            corner_radius: 18.0
            blur_level: 3.0
            edge_blur_level: 0.8
            lensing_effect: 1.0
            lensing_strength: 1.3
            lensing_width: 6.0
            tint_color: #0B1526
            tint_alpha: 0.34
            surface_alpha: 1.0
            border_alpha: 0.18
            border_width: 0.5
            rim_alpha: 0.24
            inner_shadow_alpha: 0.12
            specular_strength: 0.0
            fallback_color: #0A1424
            shadow_radius: 0.0
        }
        mod.widgets.Ticks{}
        body := View{
            width: Fill height: Fit flow: Down spacing: 9
            padding: Inset{top: 16 left: 16 right: 16 bottom: 16}
            head := View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                h := CardH{}
                View{ width: Fill height: 1 }
                idx := Tiny{}
            }
            View{ width: Fill height: Fit flow: Right spacing: 5 align: Align{y: 1.0}
                v := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "--"
                    draw_text +: { color: #FFFFFF text_style: theme.font_bold{font_size: 26.0} } }
                unit := Label{ width: Fit height: Fit padding: 0 margin: 0 text: ""
                    draw_text +: { color: #4FD8FF text_style: theme.font_code{font_size: 10.0 letter_spacing: 0.8} } }
            }
            bar := mod.widgets.Bar{}
            sub := TinyR{}
            cn := CnTiny{}
        }
    }

    // —— 风向罗盘 ——
    mod.widgets.WindRose = GaussRoundedView{
        width: 116 height: 116
        draw_bg +: {
            deg: uniform(0.0)
            lit: uniform(1.0)
            pixel: fn(){
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.circle(0.5, 0.5, 0.5)
                let p = self.pos * 2.0 - 1.0
                let d = abs(length(p))

                let base = mix(vec4(0.030, 0.062, 0.125, 1.0), vec4(0.014, 0.030, 0.065, 1.0), smoothstep(0.0, 0.5, d))
                sdf.fill_keep(base)

                let ring = smoothstep(0.36, 0.38, d) - smoothstep(0.43, 0.45, d)
                sdf.fill_keep(vec4(0.32, 0.85, 1.0, ring * 0.50))

                let cross = (1.0 - smoothstep(0.008, 0.012, abs(p.x))) + (1.0 - smoothstep(0.008, 0.012, abs(p.y)))
                sdf.fill_keep(vec4(0.30, 0.55, 0.80, cross * (1.0 - smoothstep(0.14, 0.18, d)) * 0.18))

                // `deg` 是度数；shader 的 sin/cos 吃弧度，忘了转换的话风向指针会乱指。
                let ca = cos(radians(self.deg))
                let sa = sin(radians(self.deg))
                let px = p.x * ca - p.y * sa
                let py = p.x * sa + p.y * ca
                let half = 1.0 - smoothstep(0.045, 0.056, abs(px))
                let body = half * (1.0 - smoothstep(0.16, 0.38, py))
                let tail = half * (1.0 - smoothstep(0.16, 0.20, -py))
                sdf.fill_keep(vec4(0.31, 0.85, 1.0, mix(tail, body, 0.5) * self.lit))

                sdf.fill_keep(vec4(0.55, 0.92, 1.0, 1.0 - smoothstep(0.026, 0.034, d)))

                return sdf.result
            }
        }
    }

    // —— 十天一行：图标 / 星期 / 天气 / 降水 / 温区条 ——
    let DayRow = View{
        width: Fill height: 46 flow: Right spacing: 12
        align: Align{y: 0.5}
        icon := mod.widgets.WxGlyph{ width: 26 height: 26
            svg_walk: Walk{ width: 26 height: 26 } }
        View{ width: 74 height: Fit flow: Down spacing: 3
            name := Label{ padding: 0 margin: 0 draw_text +: { color: #EAF4FF text_style: theme.font_bold{font_size: 12.0} } }
            date := Tiny{}
        }
        cond := Label{ width: 96 height: Fit padding: 0 margin: 0 max_lines: 1
            draw_text +: { color: #9DB4D4 text_style: theme.font_regular{font_size: 10.5} } }
        rain := Label{ width: 56 height: Fit padding: 0 margin: 0
            draw_text +: { color: #63799B text_style: theme.font_code{font_size: 8.5 letter_spacing: 1.0} } }
                        View{ width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
            lo := Label{ padding: 0 margin: 0 align: Align{x: 1.0 y: 0.5}
                draw_text +: { color: #7C94B4 text_style: theme.font_code{font_size: 10.0} } }
            rail := View{
                width: Fill height: 5 show_bg: true
                draw_bg +: {
                    lo: uniform(0.0)
                    hi: uniform(1.0)
                    pixel: fn(){
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 2.5)
                        let t = self.pos.x
                        let fill = step(t, self.hi) * step(self.lo, t)
                        let c = mix(vec4(0.18, 0.48, 0.94, 1.0), vec4(0.96, 0.62, 0.56, 1.0), t)
                        sdf.fill_keep(mix(vec4(0.07, 0.12, 0.21, 1.0), c, fill))
                        return sdf.result
                    }
                }
            }
            hi := Label{ padding: 0 margin: 0 align: Align{x: 0.0 y: 0.5}
                draw_text +: { color: #EAF4FF text_style: theme.font_bold{font_size: 11.0} } }
        }
    }

    mod.widgets.WeatherScreenBase = #(WeatherScreen::register_widget(vm))
    mod.widgets.WeatherScreen = set_type_default() do mod.widgets.WeatherScreenBase{
        width: Fill height: Fill
        flow: Overlay
        show_bg: true
        draw_bg +: { color: #171717 }
        // 天空底图。`CropToFill` 铺满；`visible` 先关，加载好了再开。
        atmo := Image{ width: Fill height: Fill fit: ImageFit.CropToFill visible: false }
        ScrollYView{
            width: Fill height: Fill flow: Down spacing: 14
            padding: Inset{top: 12 left: 14 right: 14 bottom: 14}
            show_bg: false
            scroll_bars +: { show_scroll_x: false }

            // 顶栏：品牌 · 坐标 · 更新时间 · LIVE
            View{ width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5}
                    View{ width: Fit height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    mod.widgets.Orb{ width: 20 height: 20 }
                    View{ width: Fit height: Fit flow: Down spacing: 1
                        Label{ padding: 0 margin: 0 width: Fit height: Fit
                            text: "知时"
                            draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 15.0 letter_spacing: 1.0} } }
                        Eyebrow{ text: "把天气变成生活建议" }
                    }
                }
                View{ width: Fill height: 1 }
                View{ width: 180 height: Fit flow: Down spacing: 3 align: Align{x: 1.0}
                    coord := Label{ width: Fill height: Fit align: Align{x: 1.0 y: 0.5}
                        draw_text +: { color: #C5C1B9 text_style: theme.font_regular{font_size: 9.0} } }
                    View{ width: Fill height: Fit flow: Right spacing: 7 align: Align{x: 1.0 y: 0.5}
                        mod.widgets.LiveDot{ width: 8 height: 8 }
                        Tiny{ text: "LIVE · AUTO" }
                    }
                }
            }

            // 实况主卡：地点 + 实况合并成一张。
            // 顶部一行：观测地点 eyebrow + 城市名 + 地区英文 + 切换按钮（点开进 PlacePickerScreen）。
            // 下面照旧：温度 / 现象 / lede / 五项指标。
            hero := HeroCard{
                mod.widgets.Ticks{}
                View{ width: Fill height: Fit flow: Down spacing: 10
                    padding: 0
                    View{ width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5}
                        View{ width: Fit height: Fit flow: Down spacing: 3
                            Eyebrow{ text: "此刻天气" }
                            View{ width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                cname := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "北京"
                                    draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 19.0 letter_spacing: 0.6} } }
                                cname_en := Ink2{}
                            }
                        }
                        View{ width: Fill height: 1 }
                                View{ width: Fit height: Fit flow: Down spacing: 3 align: Align{x: 1.0}
                            updated := Tiny{ text: "UPDATED —" align: Align{x: 1.0} }
                                place_switch := SwitchBtn{ align: Align{x: 1.0} }
                        }
                    }
                    View{ width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 1.0}
                        View{ width: Fit height: Fit flow: Overlay
                            glow := mod.widgets.Glow{ width: 180 height: 90 }
                            View{ width: Fit height: Fit flow: Right spacing: 2 align: Align{y: 1.0}
                                hero_num := HeroNum{ text: "—" }
                                hero_deg := HeroDeg{ text: "°" }
                            }
                        }
                        View{ width: Fill height: Fit flow: Down spacing: 5 align: Align{x: 0.0}
                            ccond := CondZh{ text: "—" }
                            cdate := Ink2{}
                            lede := Lede{}
                        }
                        View{ width: Fill height: 1 }
                            wx_art := WeatherIcon{ width: 66 height: 66 }
                    }
                    View{ width: Fill height: 1 show_bg: true draw_bg +: { color: #FFFFFF24 } margin: Inset{top: 0 bottom: 0} }
                    View{ width: Fill height: Fit flow: Right spacing: 0
                        View{ width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5}
                            MetaKey{ text: "体感" }
                            feels := MetaVal{ text: "—" }
                        }
                        View{ width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5}
                            MetaKey{ text: "最高 · 最低" }
                            hi_lo := MetaVal{ text: "—" }
                        }
                        View{ width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5}
                            MetaKey{ text: "日出" }
                            sunrise := MetaVal{ text: "—" }
                        }
                        View{ width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5}
                            MetaKey{ text: "日落" }
                            sunset := MetaVal{ text: "—" }
                        }
                        View{ width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5}
                            MetaKey{ text: "观测时刻" }
                            obs := MetaVal{ text: "—" }
                        }
                    }
                }
            }

            SmartCard{
                View{ width: Fill height: Fit flow: Down spacing: 7
                    View{ width: Fill height: Fit flow: Right spacing: 7 align: Align{y: 0.5}
                        Label{ padding: 0 margin: 0 width: Fit height: Fit text: "✦ AI 今日建议"
                            draw_text +: { color: #F0BD77 text_style: theme.font_bold{font_size: 13.0} } }
                        Tiny{ text: "结合实况" }
                    }
                    morning := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
                        draw_text +: { color: #F4E9D8 text_style: theme.font_regular{font_size: 12.0 line_spacing: 1.6} } }
                }
            }

            SmartCard{
                View{ width: Fill height: Fit flow: Down spacing: 9
                    View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                        Label{ padding: 0 margin: 0 width: Fit height: Fit text: "现在想做什么？"
                            draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 12.0} } }
                        View{ width: Fill height: 1 }
                        Tiny{ text: "选场景看安排 · 点按可继续问 AI" }
                    }
                    View{ width: Fill height: Fit flow: Right spacing: 7
                        ask_dry := SceneChip{
                            name.text: "晾被子"
                            draw_bg.color: #E7A95F
                            name.draw_text.color: #30251A
                            name.draw_text.text_style: theme.font_bold{font_size: 10.5}
                        }
                        ask_run := SceneChip{ name.text: "去跑步" }
                        ask_camp := SceneChip{ name.text: "露营" }
                        ask_wash := SceneChip{ name.text: "洗车" }
                    }
                    scene_answer := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 2
                        draw_text +: { color: #F1E7D8 text_style: theme.font_regular{font_size: 11.0 line_spacing: 1.5} } }
                }
            }

            View{ width: Fill height: Fit flow: Right spacing: 9
                DecisionCard{
                    width: Fill
                    View{ width: Fill height: Fit flow: Down spacing: 6
                        Label{ padding: 0 margin: 0 width: Fill height: Fit text: "今日穿搭"
                            draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 12.0} } }
                        outfit := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
                            draw_text +: { color: #D6D1C8 text_style: theme.font_regular{font_size: 10.5 line_spacing: 1.4} } }
                        Tiny{ text: "示例衣橱 · 可在我的页面完善" }
                    }
                }
                DecisionCard{
                    width: Fill
                    View{ width: Fill height: Fit flow: Down spacing: 6
                        Label{ padding: 0 margin: 0 width: Fill height: Fit text: "身体气象站"
                            draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 12.0} } }
                        bodycast := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
                            draw_text +: { color: #D6D1C8 text_style: theme.font_regular{font_size: 10.5 line_spacing: 1.4} } }
                        Tiny{ text: "天气提示，不替代医疗建议" }
                    }
                }
            }

            DecisionCard{
                cursor: MouseCursor.Hand
                View{ width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                    View{ width: 34 height: 34 align: Align{x: 0.5 y: 0.5} show_bg: true
                        draw_bg +: { color: #423528 border_radius: 17.0 }
                        Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 0 margin: 0 text: "家"
                            draw_text +: { color: #F2C98D text_style: theme.font_bold{font_size: 12.0} } }
                    }
                    View{ width: Fill height: Fit flow: Down spacing: 3
                        Label{ padding: 0 margin: 0 width: Fill height: Fit text: "跨城牵挂"
                            draw_text +: { color: #F5F1E9 text_style: theme.font_bold{font_size: 11.5} } }
                        Tiny{ width: Fill text: "添加家人所在城市，随时看看当地天气" }
                    }
                    family_add := View{ width: Fit height: 30 padding: Inset{top: 0 left: 12 right: 12 bottom: 0}
                        align: Align{x: 0.5 y: 0.5} show_bg: true cursor: MouseCursor.Hand
                        draw_bg +: { color: #453522 border_radius: 15.0 }
                        Label{ padding: 0 margin: 0 width: Fit height: Fit text: "添加城市 +"
                            draw_text +: { color: #F2C98D text_style: theme.font_bold{font_size: 9.5} } }
                    }
                }
            }

            // 24 小时温度曲线
            GlassCard{
                mod.widgets.Ticks{}
                View{ width: Fill height: Fit flow: Down spacing: 10
                    padding: Inset{top: 18 left: 22 right: 22 bottom: 18}
                    View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                        View{ width: Fit height: Fit flow: Down spacing: 3
                            Eyebrow{ text: "24H TEMPERATURE · 未来 24 小时" }
                            Label{ padding: 0 margin: 0 width: Fit height: Fit text: "Catmull-Rom 插值 · 单位 ℃"
                                draw_text +: { color: #7C94B4 text_style: theme.font_regular{font_size: 10.0} } }
                        }
                        View{ width: Fill height: 1 }
                        View{ width: Fit height: Fit flow: Down spacing: 2 align: Align{x: 1.0}
                            Label{ padding: 0 margin: 0 width: Fit height: Fit align: Align{x: 1.0 y: 0.5} text: "NOW"
                                draw_text +: { color: #4FD8FF text_style: theme.font_code{font_size: 8.0 letter_spacing: 2.0} } }
                            spark_now := Label{ padding: 0 margin: 0 width: Fit height: Fit align: Align{x: 1.0 y: 0.5} text: "—"
                                draw_text +: { color: #EAF4FF text_style: theme.font_bold{font_size: 20.0} } }
                        }
                    }
                    spark := mod.widgets.WxGlyph{ width: Fill height: 196
                        svg_walk: Walk{ width: Fill height: Fill } }
                    View{ width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5}
                        View{ width: Fit height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                            Tiny{ text: "HIGH" }
                            spark_hi := Tiny{ text: "—" }
                        }
                        View{ width: Fit height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                            Tiny{ text: "LOW" }
                            spark_lo := Tiny{ text: "—" }
                        }
                        View{ width: Fill height: 1 }
                        status := TinyR{}
                    }
                }
            }

            // 五项指标
            View{ width: Fill height: Fit flow: Right spacing: 12
                mc0 := RailCard{
                    body.head.h.text: "HUMIDITY"
                    body.head.idx.text: "01"
                    body.cn.text: "相对湿度"
                }
                mc1 := RailCard{
                    body.head.h.text: "WIND"
                    body.head.idx.text: "02"
                    body.cn.text: "10 米地面风"
                }
                mc2 := RailCard{
                    body.head.h.text: "PRESSURE"
                    body.head.idx.text: "03"
                    body.cn.text: "海平面气压"
                }
                mc3 := RailCard{
                    body.head.h.text: "VISIBILITY"
                    body.head.idx.text: "04"
                    body.cn.text: "水平能见度"
                }
                mc4 := RailCard{
                    body.head.h.text: "UV INDEX"
                    body.head.idx.text: "05"
                    body.cn.text: "紫外线指数"
                }
            }

            // 风况
            GlassCard{
                mod.widgets.Ticks{}
                View{ width: Fill height: Fit flow: Down spacing: 14
                    padding: Inset{top: 20 left: 24 right: 24 bottom: 20}
                    View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                        Eyebrow{ text: "WIND · 风况" }
                        View{ width: Fill height: 1 }
                        Tiny{ text: "SURFACE 10M" }
                    }
                    View{ width: Fill height: Fit flow: Right spacing: 22 align: Align{y: 0.5}
                        rose := mod.widgets.WindRose{}
                        View{ width: Fill height: Fit flow: Down spacing: 11 align: Align{x: 0.0}
                            View{ width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                                Tiny{ text: "DIR" }
                                wdir := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "—"
                                    draw_text +: { color: #EAF4FF text_style: theme.font_bold{font_size: 17.0} } }
                                TinyR{ width: Fill text: "风的来向" }
                            }
                            View{ width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 1.0}
                                wspeed := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "--"
                                    draw_text +: { color: #4FD8FF text_style: theme.font_bold{font_size: 40.0} } }
                                Label{ width: Fit height: Fit padding: 0 margin: 0 text: "km/h"
                                    draw_text +: { color: #7C94B4 text_style: theme.font_code{font_size: 11.0 letter_spacing: 1.0} } }
                            }
                            View{ width: Fill height: 1 show_bg: true draw_bg +: { color: #84C4FF26 } margin: Inset{top: 2 bottom: 2} }
                            View{ width: Fill height: Fit flow: Right spacing: 16 align: Align{y: 0.5}
                                View{ width: Fit height: Fit flow: Down spacing: 3
                                    Tiny{ text: "TODAY RAIN" }
                                    wprecip := Ink2{ width: Fit }
                                }
                                View{ width: Fill height: 1 }
                            }
                        }
                    }
                }
            }

            // 十天预报
            Panel{ flow: Down spacing: 2 padding: Inset{top: 18 left: 20 right: 20 bottom: 18}
                View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                    View{ width: Fit height: Fit flow: Down spacing: 3
                        Eyebrow{ text: "FORECAST · 未来 10 天" }
                        Label{ padding: 0 margin: 0 width: Fit height: Fit text: "温区条按十天区间归一"
                            draw_text +: { color: #7C94B4 text_style: theme.font_regular{font_size: 10.0} } }
                    }
                    View{ width: Fill height: 1 }
                    CnTiny{ text: "数据：Open-Meteo · 每小时更新" }
                }
                d0 := DayRow{ margin: Inset{top: 8} }
                d1 := DayRow{}
                d2 := DayRow{}
                d3 := DayRow{}
                d4 := DayRow{}
                d5 := DayRow{}
                d6 := DayRow{}
                d7 := DayRow{}
                d8 := DayRow{}
                d9 := DayRow{}
                View{ width: Fill height: 0.8 show_bg: true draw_bg +: { color: #22384C } margin: Inset{top: 10 bottom: 10} }
                CnTiny{ width: Fill text: "来源 Open-Meteo · 免 key · 时区本地" margin: Inset{top: 4} }
            }
        }
    }
}

/// 运行时拼出来的 SVG 需要一个公开的入口：`Icon` 把 `draw_icon` 留成私有字段，
/// crate 外灌不进去。这个壳只做一件事——把 `DrawSvg` 画到 `svg_walk` 上。
#[derive(Script, ScriptHook, Widget)]
pub struct WxGlyph {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[redraw]
    #[live]
    draw_icon: DrawSvg,
    #[live]
    svg_walk: Walk,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
}

impl WxGlyph {
    pub fn load(&mut self, svg: &str) {
        self.draw_icon.load_from_str(svg);
    }
}

impl Widget for WxGlyph {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, _walk: Walk) -> DrawStep {
        self.draw_icon.draw_walk(cx, self.svg_walk);
        DrawStep::done()
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct WeatherScreen {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust]
    next_frame: NextFrame,
    /// 24 小时曲线的缓存键：(取数序号, 宽, 高)。变了才重建 SVG 字符串。
    #[rust]
    spark_key: (u64, u32, u32),
    /// 天空底图灌过没有。加载是异步的，只做一次；缓存键是 `SKY_BACKDROP_PATH`。
    #[rust]
    sky_loaded: bool,
}

impl WeatherScreen {
    fn rail(&self, cx: &mut Cx, i: usize) -> WidgetRef {
        self.view.widget(cx, &[slot(&RAIL_SLOTS, i)])
    }

    /// 刷一张指标卡。`frac` 是进度条的填充比例。
    fn paint_rail_card(
        &self,
        cx: &mut Cx,
        i: usize,
        v: &str,
        unit: &str,
        sub: &str,
        cn: &str,
        frac: f64,
    ) {
        let card = self.rail(cx, i);
        card.label(cx, &[live_id!(v)]).set_text(cx, v);
        card.label(cx, &[live_id!(unit)]).set_text(cx, unit);
        card.label(cx, &[live_id!(sub)]).set_text(cx, sub);
        card.label(cx, &[live_id!(cn)]).set_text(cx, cn);
        let mut bar = card.widget(cx, &[live_id!(bar)]);
        script_apply_eval!(cx, bar, { draw_bg.frac: #(frac.clamp(0.0, 1.0) as f32) });
    }

    /// 有没有点「切换」按钮。点了就让 Shell 打开 PlacePickerScreen。
    pub fn switch_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        self.view.view(cx, ids!(place_switch)).finger_up(actions).is_some()
    }

    /// Homepage scenario prompt tapped by the user. The shell forwards it to
    /// the host assistant and opens the chat page.
    pub fn scene_prompt_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for (i, id) in [ids!(ask_dry), ids!(ask_run), ids!(ask_camp), ids!(ask_wash)].into_iter().enumerate() {
            if self.view.view(cx, id).finger_up(actions).is_some() {
                return Some(i);
            }
        }
        None
    }

    pub fn family_add_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        self.view.view(cx, ids!(family_add)).finger_up(actions).is_some()
    }

    fn hide_daily(&mut self, cx: &mut Cx) {
        for i in 0..DAY_SLOTS.len() {
            self.view.widget(cx, &[slot(&DAY_SLOTS, i)]).set_visible(cx, false);
        }
        // 让下一次取数一定会重建曲线。
        self.spark_key = (0, 0, 0);
    }

    /// 把状态推上屏。
    pub fn render(&mut self, cx: &mut Cx, st: &WeatherState) {
        // 天空底图只灌一次。加载是异步的，空窗期 `visible` 还是 false，
        // 露出外层 View 的 #04060F，不会闪成窗口 clear_color 的浅色。
        if !self.sky_loaded {
            self.sky_loaded = true;
            let sky = self.view.image(cx, ids!(atmo));
            let loaded = sky.load_image_from_data_async(
                cx, std::path::Path::new(SKY_BACKDROP_PATH),
                std::sync::Arc::new(SKY_BACKDROP),
            ).is_ok();
            sky.set_visible(cx, loaded);
        }

        // 「切换」按钮：取数中变亮青，闲时暗青。
        let busy = matches!(st.status, WxStatus::Loading);
        crate::ui::fill(
            &self.view.widget(cx, ids!(place_switch)),
            cx,
            vec4(0.220, 0.188, 0.165, 1.0),
        );
        crate::ui::tint(
            &self.view.widget(cx, ids!(place_switch)).label(cx, &[live_id!(glyph)]),
            cx,
            if busy { vec4(1.0, 0.64, 0.29, 1.0) } else { vec4(0.96, 0.64, 0.31, 1.0) },
        );

        let city = st.city();
        self.view.label(cx, ids!(cname)).set_text(cx, city.name.as_ref());
        self.view.label(cx, ids!(cname_en)).set_text(cx, city.en.as_ref());
        self.view.label(cx, ids!(coord)).set_text(
            cx,
            &format!("N {:.2}°   E {:.2}°", city.lat, city.lon),
        );
        let status = self.view.label(cx, ids!(status));
        status.set_text(cx, &screen_status(&st.status));
        crate::ui::tint(&status, cx, vec4(0.486, 0.580, 0.694, 1.0));

        match &st.forecast {
            None => self.paint_empty(cx, st),
            Some(f) => self.paint_forecast(cx, st, f),
        }
        self.view.redraw(cx);
    }

    /// 没有数据：把各块的占位文字摆好，背景留一片暗下去的流场。
    fn paint_empty(&mut self, cx: &mut Cx, st: &WeatherState) {
        self.view.label(cx, ids!(updated)).set_text(cx, "UPDATED —");
        self.view.label(cx, ids!(ccond)).set_text(cx, empty_condition(&st.status));
        self.view.label(cx, ids!(cdate)).set_text(cx, &date_zh(&model::today_local()));
        self.view.label(cx, ids!(lede)).set_text(cx, "正在等待首个观测值…");
        self.view.label(cx, ids!(hero_num)).set_text(cx, "—");
        self.view.label(cx, ids!(feels)).set_text(cx, "—");
        self.view.label(cx, ids!(hi_lo)).set_text(cx, "—");
        self.view.label(cx, ids!(sunrise)).set_text(cx, "—");
        self.view.label(cx, ids!(sunset)).set_text(cx, "—");
        self.view.label(cx, ids!(obs)).set_text(cx, "—");
        self.view.label(cx, ids!(morning)).set_text(cx, "正在读取当地天气，为你整理今天的出行建议…");
        self.view.label(cx, ids!(scene_answer)).set_text(cx, "选择一个场景，向 AI 询问更具体的时间安排。");
        self.view.label(cx, ids!(outfit)).set_text(cx, "天气更新后生成今日穿搭建议。");
        self.view.label(cx, ids!(bodycast)).set_text(cx, "天气数据更新后显示紫外线与体感提醒。");
        self.view.label(cx, ids!(wdir)).set_text(cx, "—");
        self.view.label(cx, ids!(wspeed)).set_text(cx, "--");
        self.view.label(cx, ids!(wprecip)).set_text(cx, "无");
        self.view.label(cx, ids!(spark_now)).set_text(cx, "—");
        self.view.label(cx, ids!(spark_hi)).set_text(cx, "—");
        self.view.label(cx, ids!(spark_lo)).set_text(cx, "—");
        for i in 0..RAIL_SLOTS.len() {
            self.paint_rail_card(cx, i, "—", "", "—", "—", 0.0);
        }
        self.paint_wind(cx, 0.0, false);
        self.hide_daily(cx);
    }

    fn paint_forecast(&mut self, cx: &mut Cx, st: &WeatherState, f: &Forecast) {
        let cur = f.current.as_ref();
        let code = cur.and_then(|c| c.weather_code).unwrap_or(-1);
        let (_warm, lit) = weather_mood(code);
        let hum = cur.and_then(|c| c.relative_humidity_2m).unwrap_or(50.0);
        let hour = observation_hour(cur.and_then(|c| c.time.as_deref()));
        let is_day = cur.and_then(|c| c.is_day).map(|v| v != 0).unwrap_or((7..19).contains(&hour));
        let (sky_top, sky_bottom) = hero_sky(code, is_day, hour);
        let mut hero = self.view.widget(cx, ids!(hero));
        script_apply_eval!(cx, hero, {
            draw_bg.sky_top: #(sky_top)
            draw_bg.sky_bottom: #(sky_bottom)
            draw_bg.sunlight: #(if is_day { lit as f32 } else { 0.22f32 })
        });
        if let Some(t) = cur.and_then(|c| c.time.as_ref()) {
            self.view.label(cx, ids!(cdate)).set_text(cx, &date_zh(model::date_of(t)));
            self.view.label(cx, ids!(obs)).set_text(cx, model::hhmm(t));
            self.view.label(cx, ids!(updated)).set_text(cx, &format!("当地观测 {}", model::hhmm(t)));
        }
        self.view.label(cx, ids!(ccond)).set_text(cx, model::condition(code));
        crate::ui::tint(&self.view.label(cx, ids!(ccond)), cx, condition_ink(code));
        let kind = weather_art_kind(code);
        let mut art = self.view.widget(cx, ids!(wx_art));
        script_apply_eval!(cx, art, { draw_bg.cond: #(kind) draw_bg.night: #(if is_day { 0.0f32 } else { 1.0f32 }) });
        let glow = 0.32 + lit * 0.24;
        let mut glow_w = self.view.widget(cx, ids!(hero)).widget(cx, &[live_id!(hero), live_id!(glow)]);
        script_apply_eval!(cx, glow_w, { draw_bg.amp: #(glow) });

        let temp = cur
            .and_then(|c| c.temperature_2m)
            .map(|v| format!("{:.0}", v.round()))
            .unwrap_or_else(|| "—".into());
        self.view.label(cx, ids!(hero_num)).set_text(cx, &temp);

        let high_low = f.daily.as_ref().and_then(|d| {
            let today = model::today_local();
            let i = d.time.iter().position(|date| date == &today).unwrap_or(0);
            Some((d.temperature_2m_max.get(i).copied()?, d.temperature_2m_min.get(i).copied()?))
        });
        let condition = model::condition(code);
        let wind = cur.and_then(|c| c.wind_speed_10m).map(|v| format!("，风速约 {:.0} 公里/小时", v)).unwrap_or_default();
        let range = high_low.map(|(hi, lo)| format!("最高 {:.0}°，最低 {:.0}°", hi, lo)).unwrap_or_else(|| "天气数据正在更新".into());
        let advice = if cur.and_then(|c| c.precipitation).unwrap_or(0.0) > 0.1 || matches!(code, 51..=67 | 80..=82 | 95..=99) {
            format!("今天{condition}，出门带伞更稳妥。{range}{wind}。")
        } else if cur.and_then(|c| c.wind_speed_10m).unwrap_or(0.0) >= 25.0 {
            format!("今天{condition}，风有些大，外套拉链记得拉好。{range}{wind}。")
        } else {
            format!("今天{condition}，适合轻装出门。{range}{wind}。")
        };
        self.view.label(cx, ids!(morning)).set_text(cx, &advice);
        let scene_answer = if matches!(code, 51..=67 | 80..=82 | 95..=99)
            || cur.and_then(|c| c.precipitation).unwrap_or(0.0) > 0.1
        {
            "晾被子先等等：当前有降水，建议等雨停、湿度下降后再安排。"
        } else if cur.and_then(|c| c.wind_speed_10m).unwrap_or(0.0) >= 25.0 {
            "晾被子可选午前的短时晴窗；风偏大，记得固定好衣物。"
        } else if hum >= 78.0 {
            "晾被子建议选日照较好的中午时段；空气偏潮，留意回收时间。"
        } else {
            "适合安排晾晒：优先选上午到午后，日照更足、衣物干得更快。"
        };
        self.view.label(cx, ids!(scene_answer)).set_text(cx, scene_answer);
        let outfit = high_low.map(|(hi, lo)| {
            if lo < 8.0 { "保暖外套 + 长裤 + 适合步行的鞋" }
            else if hi > 28.0 { "轻薄上衣 + 透气长裤，午后注意防晒" }
            else if lo < 16.0 { "薄卫衣 + 直筒长裤 + 轻便鞋" }
            else { "长袖上衣 + 轻便长裤" }
        }).unwrap_or("根据今天的温度，为你推荐轻便舒适的搭配");
        self.view.label(cx, ids!(outfit)).set_text(cx, outfit);
        let uv_series = f.hourly.as_ref().map(|h| h.uv_index.clone());
        let uv = pick(&uv_series, cur_index(f));
        let bodycast = uv.map(|v| if v >= 6.0 { "紫外线偏强，户外活动记得做好防晒。" } else if v >= 3.0 { "紫外线中等，长时间户外可考虑防晒。" } else { "当前紫外线较低，外出体感以温度为主。" }).unwrap_or("关注紫外线、气温与湿度变化，按需安排户外活动。");
        self.view.label(cx, ids!(bodycast)).set_text(cx, bodycast);

        let feels = cur.and_then(|c| c.apparent_temperature);
        self.view.label(cx, ids!(feels)).set_text(cx, &fmt(feels, "°"));
        self.view.label(cx, ids!(lede)).set_text(
            cx,
            &format!(
                "{} · 体感 {} · 湿度 {} · 云量 {}",
                model::condition(code),
                fmt(feels, "°"),
                fmt(Some(hum), "%"),
                fmt(cur.and_then(|c| c.cloud_cover), "%")
            ),
        );

        // 五项指标卡。露点/气压/能见度/UV/阵风都从 hourly 序列取当前那一格。
        let ci = cur_index(f);
        let dew = pick(&f.hourly.as_ref().map(|h| h.dew_point_2m.clone()), ci);
        let pres = pick(&f.hourly.as_ref().map(|h| h.pressure_msl.clone()), ci);
        let vis = pick(&f.hourly.as_ref().map(|h| h.visibility.clone()), ci);
        let uv = pick(&f.hourly.as_ref().map(|h| h.uv_index.clone()), ci);
        let gust = pick(&f.hourly.as_ref().map(|h| h.wind_gusts_10m.clone()), ci);
        let wsp = cur.and_then(|c| c.wind_speed_10m);
        let wdir = cur.and_then(|c| c.wind_direction_10m);
        let cloud = cur.and_then(|c| c.cloud_cover);

        let now_p = pick(
            &f.hourly.as_ref().map(|h| h.pressure_msl.clone()),
            ci,
        );
        let prev_p = if ci > 0 {
            pick(&f.hourly.as_ref().map(|h| h.pressure_msl.clone()), ci - 1)
        } else {
            None
        };
        let dp = match (now_p, prev_p) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        };

        self.paint_rail_card(
            cx,
            0,
            &fmt(Some(hum), ""),
            "%",
            &format!("DEW POINT 露点 {}", fmt1(dew, "°")),
            "相对湿度",
            hum / 100.0,
        );
        self.paint_rail_card(
            cx,
            1,
            &fmt(wsp, ""),
            "km/h",
            &format!("{}  · GUST {}", wdir.map(compass_zh).unwrap_or("—"), fmt(gust, "")),
            "10 米地面风",
            wsp.unwrap_or(0.0) / 60.0,
        );
        self.paint_rail_card(
            cx,
            2,
            &fmt1(pres, ""),
            "hPa",
            &format!("24H TREND {} {}", trend_arrow(dp.unwrap_or(0.0)), fmt_signed(dp, " hPa")),
            "海平面气压",
            (pres.unwrap_or(1013.0) - 990.0) / 50.0,
        );
        let vis_km = vis.map(|v| v / 1000.0);
        self.paint_rail_card(
            cx,
            3,
            &vis_km.map(|v| format!("{v:.1}")).unwrap_or_else(|| "—".into()),
            "km",
            &format!("CLOUD COVER 云量 {}", fmt(cloud, "%")),
            "水平能见度",
            vis_km.unwrap_or(0.0) / 30.0,
        );
        self.paint_rail_card(
            cx,
            4,
            &fmt1(uv, ""),
            "UV",
            &format!("LEVEL {}", uv.map(uv_word).unwrap_or("—")),
            "紫外线指数",
            uv.unwrap_or(0.0) / 11.0,
        );

        // 罗盘指针：没有风向数据时只画表盘。
        self.paint_wind(cx, wdir.unwrap_or(0.0), wdir.is_some());
        self.view.label(cx, ids!(wdir)).set_text(cx, wdir.map(compass_zh).unwrap_or("—"));
        self.view.label(cx, ids!(wspeed)).set_text(cx, &fmt(wsp, ""));

        let Some(d) = &f.daily else {
            self.view.label(cx, ids!(hi_lo)).set_text(cx, "—");
            self.view.label(cx, ids!(sunrise)).set_text(cx, "—");
            self.view.label(cx, ids!(sunset)).set_text(cx, "—");
            self.view.label(cx, ids!(wprecip)).set_text(cx, "无");
            self.hide_daily(cx);
            return;
        };

        let now = model::today_local();
        let i = d.time.iter().position(|x| x == &now).unwrap_or(0);
        if i < d.time.len() {
            let hi = d.temperature_2m_max.get(i).map(|v| format!("{:.0}°", v.round())).unwrap_or_else(|| "—".into());
            let lo = d.temperature_2m_min.get(i).map(|v| format!("{:.0}°", v.round())).unwrap_or_else(|| "—".into());
            self.view.label(cx, ids!(hi_lo)).set_text(cx, &format!("{hi} / {lo}"));
            if let Some(sr) = d.sunrise.get(i) {
                self.view.label(cx, ids!(sunrise)).set_text(cx, model::hhmm(sr));
            }
            if let Some(ss) = d.sunset.get(i) {
                self.view.label(cx, ids!(sunset)).set_text(cx, model::hhmm(ss));
            }
            self.view.label(cx, ids!(wprecip)).set_text(cx, &fmt_mm(d.precipitation_sum.get(i).copied()));
        }

        self.paint_spark(cx, st, f);
        self.paint_daily(cx, d);
    }

    /// 24 小时温度曲线。SVG 按控件实测尺寸构造，`meet` 才不会被缩放留边。
    fn paint_spark(&mut self, cx: &mut Cx, st: &WeatherState, f: &Forecast) {
        let Some(h) = &f.hourly else {
            self.view.label(cx, ids!(spark_now)).set_text(cx, "—");
            return;
        };
        let temps: Vec<f64> = h.temperature_2m.iter().take(24).copied().collect();
        if temps.len() < 2 {
            self.view.label(cx, ids!(spark_now)).set_text(cx, "—");
            return;
        }
        let (lo, hi) = temps.iter().fold((f64::MAX, f64::MIN), |(a, b), t| {
            (a.min(*t), b.max(*t))
        });
        let ci = cur_index(f);
        self.view.label(cx, ids!(spark_now)).set_text(
            cx,
            &temps.get(ci.min(temps.len() - 1))
                .map(|v| format!("{:.0}°", v.round()))
                .unwrap_or_else(|| "—".into()),
        );
        self.view.label(cx, ids!(spark_hi)).set_text(cx, &format!("{:.0}°", hi.round()));
        self.view.label(cx, ids!(spark_lo)).set_text(cx, &format!("{:.0}°", lo.round()));

        let spark = self.view.widget(cx, ids!(spark));
        let r = spark.area().rect(cx);
        let w = r.size.x.max(40.0) as u32;
        let hh = r.size.y.max(60.0) as u32;
        let key = (st.seq, w, hh);
        if key == self.spark_key {
            return;
        }
        self.spark_key = key;
        let svg = spark_svg(&temps, w, hh, ci.min(temps.len() - 1));
        if svg.is_empty() {
            return;
        }
        if let Some(mut g) = self.view.widget(cx, ids!(spark)).borrow_mut::<WxGlyph>() {
            g.load(&svg);
        }
    }

    fn paint_wind(&self, cx: &mut Cx, deg: f64, lit: bool) {
        let rose = self.view.widget(cx, ids!(rose));
        let mut rose = rose;
        script_apply_eval!(
            cx,
            rose,
            { draw_bg.deg: #((deg as f32).rem_euclid(360.0) as f32) draw_bg.lit: #(if lit { 1.0 } else { 0.0 }) }
        );
    }

    fn paint_daily(&mut self, cx: &mut Cx, d: &model::Daily) {
        let n = d.time.len().min(DAY_SLOTS.len());
        if n == 0 {
            self.hide_daily(cx);
            return;
        }
        // 十天区间，用来把温区条归一到同一条尺度上。
        let (mut lo_all, mut hi_all) = (f64::MAX, f64::MIN);
        for i in 0..n {
            lo_all = lo_all.min(*d.temperature_2m_min.get(i).unwrap_or(&0.0));
            hi_all = hi_all.max(*d.temperature_2m_max.get(i).unwrap_or(&0.0));
        }
        let span = (hi_all - lo_all).max(1.0);
        let today = model::today_local();
        for i in 0..DAY_SLOTS.len() {
            let row = self.view.widget(cx, &[slot(&DAY_SLOTS, i)]);
            let live = i < n;
            row.set_visible(cx, live);
            if !live {
                continue;
            }
            let date = d.time[i].as_str();
            let mo = date.split('-').nth(1).unwrap_or("");
            let md = date.split('-').nth(2).unwrap_or("").trim_start_matches('0');
            row.label(cx, &[live_id!(name)]).set_text(cx, &day_name(date));
            row.label(cx, &[live_id!(date)]).set_text(cx, &format!("{mo}-{md}"));
            let code = *d.weather_code.get(i).unwrap_or(&-1);
            row.label(cx, &[live_id!(cond)]).set_text(cx, model::condition(code));
            crate::ui::tint(
                &row.label(cx, &[live_id!(cond)]),
                cx,
                condition_ink(code),
            );
            row.label(cx, &[live_id!(rain)]).set_text(cx, &fmt_mm(d.precipitation_sum.get(i).copied()));
            let lo = d.temperature_2m_min.get(i).copied().unwrap_or(0.0).round() as i32;
            let hi = d.temperature_2m_max.get(i).copied().unwrap_or(0.0).round() as i32;
            row.label(cx, &[live_id!(lo)]).set_text(cx, &format!("{lo}°"));
            row.label(cx, &[live_id!(hi)]).set_text(cx, &format!("{hi}°"));
            let l = ((lo as f64 - lo_all) / span).clamp(0.0, 1.0) as f32;
            let r = ((hi as f64 - lo_all) / span).clamp(0.0, 1.0) as f32;
            let mut rail = row.widget(cx, &[live_id!(rail)]);
            script_apply_eval!(cx, rail, { draw_bg.lo: #(l) draw_bg.hi: #(r) });
            // 图标：今天用亮青，其余压暗。
            let accent = if date == today { "#4FD8FF" } else { "#7C94B4" };
            if let Some(mut ic) = row.widget(cx, &[live_id!(icon)]).borrow_mut::<WxGlyph>() {
                ic.load(&glyph_svg(code, accent));
            }
        }
    }
}

impl Widget for WeatherScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if self.next_frame.is_event(event).is_some() {
            self.view.redraw(cx);
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.next_frame = cx.new_next_frame();
        self.view.draw_walk(cx, scope, walk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_starts_empty_on_the_first_city() {
        let st = WeatherState::new();
        assert_eq!(st.city(), &CITIES[0]);
        assert!(!st.has_data());
        assert!(matches!(st.status, WxStatus::Idle));
    }

    #[test]
    fn city_selection_reports_only_real_changes() {
        let mut st = WeatherState::new();
        assert!(!st.select(0));
        assert!(st.select(3));
        assert_eq!(st.city(), &CITIES[3]);
        assert!(!st.select(3));
    }

    #[test]
    fn out_of_range_selection_clamps_to_the_last_city() {
        let mut st = WeatherState::new();
        st.select(999);
        assert_eq!(st.city(), &CITIES[CITIES.len() - 1]);
    }

    #[test]
    fn the_slot_lookup_wraps_to_the_last_slot() {
        assert_eq!(slot(&DAY_SLOTS, 3), LiveId::from_str("d3"));
        assert_eq!(slot(&DAY_SLOTS, 40), LiveId::from_str("d9"));
        assert_eq!(slot(&RAIL_SLOTS, 4), LiveId::from_str("mc4"));
        assert_eq!(slot(&RAIL_SLOTS, 99), LiveId::from_str("mc4"));
    }

    #[test]
    fn the_day_name_labels_today_and_tomorrow() {
        assert_eq!(day_name(&model::today_local()), "今天");
        assert_eq!(day_name(&model::next_local_date(1)), "明天");
        assert_eq!(
            day_name(&model::next_local_date(3)),
            model::weekday(&model::next_local_date(3))
        );
    }

    #[test]
    fn the_date_zh_reads_chinese() {
        assert!(date_zh("2026-09-27").starts_with("9月27日"));
        assert_eq!(date_zh("junk"), "junk");
    }

    #[test]
    fn the_status_lines_are_not_empty() {
        assert!(!status_line(&WxStatus::Idle).is_empty());
        assert!(!status_line(&WxStatus::Loading).is_empty());
        assert!(!status_line(&WxStatus::Done).is_empty());
        assert!(status_line(&WxStatus::Error("x".into())).contains("x"));
    }

    /// 取数成功后不能再读「还没取数」：`resolve` 把状态改成 `Done`，脚注跟着变。
    #[test]
    fn a_successful_fetch_no_longer_reads_as_not_fetched() {
        let mut st = WeatherState::new();
        st.status = WxStatus::Done;
        assert!(!st.status_text().contains("还没取数"));
        assert!(st.status_text().contains("已更新"));
    }

    /// 实况标题位同样不能出现「还没取数」这种传输层措辞。
    #[test]
    fn the_empty_condition_never_leaks_transport_wording() {
        for s in [WxStatus::Idle, WxStatus::Loading, WxStatus::Done] {
            let text = empty_condition(&s);
            assert!(!text.is_empty());
            assert!(!text.contains("取数"));
            assert!(!text.contains("还没"));
        }
        assert_eq!(empty_condition(&WxStatus::Error("x".into())), "连接暂不可用");
    }

    #[test]
    fn the_compass_reads_the_eight_chinese_dirs() {
        for (deg, name) in [
            (0.0, "北"),
            (45.0, "东北"),
            (90.0, "东"),
            (135.0, "东南"),
            (180.0, "南"),
            (225.0, "西南"),
            (270.0, "西"),
            (315.0, "西北"),
        ] {
            assert_eq!(compass_zh(deg), name, "deg={deg}");
        }
        // 过界和负角都能回到正确的方位。
        assert_eq!(compass_zh(360.0), "北");
        assert_eq!(compass_zh(720.0), "北");
        assert_eq!(compass_zh(-90.0), "西");
    }

    #[test]
    fn the_value_formatters_fallback_to_placeholders() {
        assert_eq!(fmt(None, "%"), "—");
        assert_eq!(fmt(Some(63.4), "%"), "63%");
        assert_eq!(fmt1(None, "hPa"), "—");
        assert_eq!(fmt1(Some(1012.34), "hPa"), "1012.3hPa");
        assert_eq!(fmt_signed(Some(1.2), " hPa"), "+1.2 hPa");
        assert_eq!(fmt_signed(Some(-0.4), " hPa"), "-0.4 hPa");
        assert_eq!(fmt_mm(None), "无");
        assert_eq!(fmt_mm(Some(0.0)), "无");
        assert_eq!(fmt_mm(Some(0.6)), "0.6mm");
    }

    #[test]
    fn the_uv_levels_follow_the_who_bands() {
        assert_eq!(uv_word(1.0), "低");
        assert_eq!(uv_word(2.9), "低");
        assert_eq!(uv_word(3.0), "中等");
        assert_eq!(uv_word(5.9), "中等");
        assert_eq!(uv_word(6.0), "高");
        assert_eq!(uv_word(7.9), "高");
        assert_eq!(uv_word(8.0), "很高");
        assert_eq!(uv_word(11.0), "极高");
        assert_eq!(uv_word(99.0), "极高");
    }

    #[test]
    fn the_trend_arrow_reads_direction() {
        assert_eq!(trend_arrow(0.3), "↑");
        assert_eq!(trend_arrow(-0.3), "↓");
        assert_eq!(trend_arrow(0.0), "→");
    }

    #[test]
    fn pick_reads_the_optional_series() {
        let s = Some(vec![1.0, 2.0, 3.0]);
        assert_eq!(pick(&s, 1), Some(2.0));
        assert_eq!(pick(&s, 99), None);
        let none: Option<Vec<f64>> = None;
        assert_eq!(pick(&none, 0), None);
    }

    #[test]
    fn the_glyph_svg_is_well_formed() {
        for code in [0i32, 1, 2, 3, 45, 48, 63, 75, 82, 96, -1, 999] {
            let svg = glyph_svg(code, "#4FD8FF");
            assert!(svg.starts_with("<svg"), "code={code}");
            assert!(svg.contains("</svg>"), "code={code}");
            assert!(svg.contains("viewBox=\"0 0 26 26\""), "code={code}");
            assert!(svg.contains("#4FD8FF"), "code={code}");
        }
    }

    #[test]
    fn the_spark_svg_builds_a_closed_area_and_a_now_marker() {
        let temps: Vec<f64> = (0..24).map(|i| 20.0 + i as f64 * 0.3).collect();
        let svg = spark_svg(&temps, 900, 196, 13);
        assert!(!svg.is_empty());
        assert!(svg.contains("<linearGradient id=\"ar\""));
        assert!(svg.contains("<linearGradient id=\"ln\""));
        assert!(svg.contains("viewBox=\"0 0 900 196\""));
        assert!(svg.contains("stroke-dasharray=\"3 3\""));
        assert!(svg.contains("url(#ar)"));
        assert!(svg.contains("url(#ln)"));
        // 24 个点 → 23 段贝塞尔；曲线同时用在描边和填色路径上，所以是两倍。
        let beziers = svg.matches(" C ").count();
        assert_eq!(beziers, 23 * 2);
        // 干数据也要能出图。
        let flat: Vec<f64> = vec![20.0; 24];
        assert!(!spark_svg(&flat, 900, 196, 0).is_empty());
        // 太小或太少点直接不出图。
        assert!(spark_svg(&temps, 5, 5, 0).is_empty());
        assert!(spark_svg(&temps[..1].to_vec(), 900, 196, 0).is_empty());
    }

    #[test]
    fn the_weather_mood_stays_in_range() {
        for code in [-1i32, 0, 2, 3, 45, 63, 75, 96, 999] {
            let (warm, lit) = weather_mood(code);
            assert!(warm >= 0.0 && warm <= 1.0, "warm code={code}");
            assert!(lit >= 0.0 && lit <= 1.0, "lit code={code}");
        }
    }
}

// 外壳：今天 / 天空 / 问答 / 我的四个页签，外加全屏地区切换页。
//
// 天气、对话、日程状态和宿主 AI 客户端都放在这里，页签之间共享。
// page_picker 是天气页的覆盖层，亮起时把 nav 藏起来，让用户专心选地区。
use makepad_widgets::*;
use octosense_app_peers::OctosAppService;
use std::sync::Arc;

use crate::agent::{AgentClient, AgentUpdate};
use crate::console::{self, ChatLog, ConsoleScreen};
use crate::gallery::SkyGallery;
use crate::nav::{NavHit, NavBar};
use crate::place_picker::PlacePickerScreen;
use crate::schedule::ScheduleState;
use crate::weather::{WeatherState, WxStatus};
use crate::today::TodayScreen;
use crate::mine::MineScreen;
use crate::mine::WeatherProfile;
use makepad_widgets::makepad_platform::storage::{StorageHandle, StorageRequestId, StorageResponse, StorageResult};

fn care_weather_draft(contact: &crate::mine::CareContact, forecast: &crate::model::Forecast) -> String {
    let city = contact.city.name.as_ref();
    let to = if contact.name.trim().is_empty() { contact.relationship.trim() } else { contact.name.trim() };
    let greeting = if to.is_empty() { "".to_string() } else { format!("{to}，") };
    let detail = contact.personal_details.trim();
    let personal = if detail.is_empty() { String::new() } else { format!("{detail}。") };
    let daily = forecast.daily.as_ref();
    let current_date = forecast.current.as_ref().and_then(|c| c.time.as_deref()).and_then(|time| time.get(..10));
    let day = daily.and_then(|d| current_date.and_then(|date| d.time.iter().position(|candidate| candidate == date)).or(Some(0)));
    let high = day.and_then(|i| daily?.temperature_2m_max.get(i)).copied();
    let low = day.and_then(|i| daily?.temperature_2m_min.get(i)).copied();
    if let (Some(high), Some(low)) = (high, low) {
        let drop = (high - low).round() as i32;
        if drop >= 6 {
            let close = match contact.tone.as_str() { "轻松" => "外套记得多带一件哈。", "简短" => "记得加衣。", _ => "出门记得加件外套。" };
            return format!("{greeting}{city}今晚最低约 {:.0}°C，比白天最高温低 {drop}°C。{personal}{close}", low);
        }
    }
    if let Some(hourly) = forecast.hourly.as_ref() {
        let start = forecast.current.as_ref().and_then(|c| c.time.as_deref()).map(|time| crate::model::current_index(hourly, time)).unwrap_or(0);
        let end = (start + 12).min(hourly.precipitation_probability.len());
        let rain = hourly.precipitation_probability.get(start..end).and_then(|values| values.iter().copied().reduce(f64::max));
        if let Some(probability) = rain.filter(|p| *p >= 60.0) {
            let close = match contact.tone.as_str() { "轻松" => "别忘了带伞呀。", "简短" => "记得带伞。", _ => "出门前记得带伞。" };
            return format!("{greeting}{city}接下来 12 小时降雨概率最高约 {:.0}%。{personal}{close}", probability);
        }
    }
    if let (Some(high), Some(low)) = (high, low) {
        let condition = forecast.current.as_ref().and_then(|c| c.weather_code).map(crate::model::condition).unwrap_or("天气");
        format!("{greeting}{city}今天{condition}，气温约 {:.0}–{:.0}°C。{personal}出门前看看体感，按需添衣。", low, high)
    } else {
        format!("{greeting}我看了{}今天的天气，出门前记得留意温度变化。{personal}", city)
    }
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellBase = #(Shell::register_widget(vm))
    mod.widgets.Shell = set_type_default() do mod.widgets.ShellBase{
        width: Fill
        height: Fill
        flow: Down
        spacing: 0
        show_bg: true
        draw_bg +: { color: #191919 }

        page_weather := TodayScreen{ width: Fill height: Fill }
        page_gallery := SkyGallery{ width: Fill height: Fill visible: false }
        page_console := ConsoleScreen{ width: Fill height: Fill visible: false }
        page_me := MineScreen{ width: Fill height: Fill visible: false }
        // 覆盖层：只在 picker_open 时亮。流式是 Down 的最后一个，靠 `visible` 切换。
        page_picker := PlacePickerScreen{ width: Fill height: Fill visible: false }

        nav := NavBar{}
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct Shell {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust(NavHit::Weather)]
    tab: NavHit,
    #[rust(WeatherState::new())]
    wx: WeatherState,
    #[rust(ChatLog::new())]
    chat: ChatLog,
    #[rust(ScheduleState::new())]
    schedule: ScheduleState,
    #[rust(WeatherProfile::default())]
    profile: WeatherProfile,
    #[rust]
    profile_storage: Option<StorageHandle>,
    #[rust]
    profile_load: Option<StorageRequestId>,
    #[rust(Vec::new())]
    care_flights: Vec<(LiveId, crate::model::City)>,
    #[rust(0u64)]
    care_seq: u64,
    #[rust(Timer::empty())]
    care_poll: Timer,
    #[rust]
    care_agent: Option<AgentClient>,
    #[rust(Timer::empty())]
    care_ai_poll: Timer,
    #[rust]
    care_ai_target: Option<(usize, String)>,
    /// 是否正在覆盖式展示地区切换页。
    #[rust(false)]
    picker_open: bool,
    /// 缓存的「模型标签」：当 agent 还没建出来时也要给个明确说法，不能让
    /// 用户对着「OctoSense 系统 AI」假象发消息反复碰壁。
    #[rust(String::from("AI 助手未配置"))]
    model_label_cache: String,
    #[rust]
    agent: Option<AgentClient>,
    #[rust]
    assistant: Option<Arc<dyn OctosAppService>>,
    #[rust(String::new())]
    instance_scope: String,
    #[rust(false)]
    initial_console_rendered: bool,
    /// Event signals can be consumed by the host before reaching an embedded
    /// module root, so poll the async reply channel while a turn is active.
    #[rust(Timer::empty())]
    agent_poll: Timer,
}

impl Shell {
    pub fn set_storage(&mut self, cx: &mut Cx, storage: StorageHandle) {
        self.profile_load = Some(storage.get(cx, "weather-agent/profile.json"));
        self.profile_storage = Some(storage);
    }

    fn save_profile(&self, cx: &mut Cx) {
        let Some(storage) = self.profile_storage.as_ref() else { return; };
        let cities = self.profile.family_cities.iter().map(|c| serde_json::json!({"name":c.name,"en":c.en,"lat":c.lat,"lon":c.lon})).collect::<Vec<_>>();
        let care_contacts = self.profile.care_contacts.iter().map(|c| serde_json::json!({
            "city": {"name":c.city.name,"en":c.city.en,"lat":c.city.lat,"lon":c.city.lon},
            "name":c.name,"relationship":c.relationship,"matrix_room":c.matrix_room,
            "personal_details":c.personal_details,"tone":c.tone,
        })).collect::<Vec<_>>();
        let care_reminders = self.profile.care_reminders.iter().map(|r| serde_json::json!({
            "contact_index":r.contact_index,"event_key":r.event_key,"date":r.date,"created_at":r.created_at,"fact":r.fact,"draft":r.draft,"sent":r.sent,
        })).collect::<Vec<_>>();
        let value = serde_json::json!({"wardrobe":self.profile.wardrobe,"factors":self.profile.factors,"family_cities":cities,"care_contacts":care_contacts,"care_reminders":care_reminders});
        if let Ok(bytes) = serde_json::to_vec(&value) { storage.set(cx, "weather-agent/profile.json", bytes); }
    }

    fn refresh_care_weather(&mut self, cx: &mut Cx) {
        let mut requested = Vec::<(f64, f64)>::new();
        for contact in self.profile.care_contacts.clone() {
            if requested.iter().any(|(lat, lon)| *lat == contact.city.lat && *lon == contact.city.lon) { continue; }
            if self.care_flights.iter().any(|(_, city)| city.lat == contact.city.lat && city.lon == contact.city.lon) { continue; }
            requested.push((contact.city.lat, contact.city.lon));
            self.care_seq = self.care_seq.wrapping_add(1);
            let request_id = LiveId::from_str(&format!("care_wx_{}", self.care_seq));
            self.care_flights.push((request_id, contact.city.clone()));
            // The care monitor also needs yesterday's precipitation to avoid
            // repeating a rain reminder when the recipient already had rain.
            let url = crate::model::forecast_url(&contact.city).replace("&timezone=auto", "&past_days=1&timezone=auto");
            let mut request = HttpRequest::new(url, HttpMethod::GET);
            request.set_header("Accept".into(), "application/json".into());
            cx.http_request(request_id, request);
        }
    }

    fn resolve_care_weather(&mut self, cx: &mut Cx, request_id: LiveId, result: Result<HttpResponse, String>) {
        let Some(position) = self.care_flights.iter().position(|(id, _)| *id == request_id) else { return; };
        let (_, city) = self.care_flights.remove(position);
        let forecast = result.ok().and_then(|response| {
            if response.status_code != 200 { return None; }
            let body = response.get_body()?;
            if body.len() > 1_000_000 { return None; }
            crate::model::parse_forecast(std::str::from_utf8(body).ok()?).ok()
        });
        let Some(forecast) = forecast else { return; };
        let current_time = forecast.current.as_ref().and_then(|c| c.time.as_deref()).unwrap_or("");
        let local_hour = current_time.get(11..13).and_then(|s| s.parse::<u32>().ok()).unwrap_or(12);
        let Some(daily) = forecast.daily.as_ref() else { return; };
        let today = current_time.get(..10).and_then(|date| daily.time.iter().position(|d| d == date)).unwrap_or(0);
        let next = today + 1;
        let temperature_event = if (20..=21).contains(&local_hour) {
            match (
                daily.temperature_2m_max.get(today), daily.temperature_2m_max.get(next),
                daily.temperature_2m_min.get(today), daily.temperature_2m_min.get(next),
            ) {
                (Some(today_high), Some(next_high), Some(today_low), Some(next_low)) => {
                    let high_drop = (*today_high - *next_high).round() as i32;
                    let low_drop = (*today_low - *next_low).round() as i32;
                    if high_drop >= 6 { Some(("最高温", high_drop, *next_high, next)) }
                    else if low_drop >= 6 { Some(("最低温", low_drop, *next_low, next)) }
                    else { None }
                }
                _ => None,
            }
        } else { None };
        let rain_event = if (16..=18).contains(&local_hour) {
            let yesterday_dry = today.checked_sub(1)
                .and_then(|i| daily.precipitation_sum.get(i))
                .is_some_and(|amount| *amount < 0.1);
            let rain_probability = forecast.hourly.as_ref().and_then(|hourly| {
                let start = crate::model::current_index(hourly, current_time);
                hourly.precipitation_probability.get(start..(start + 12).min(hourly.precipitation_probability.len()))
                    .and_then(|values| values.iter().copied().reduce(f64::max))
            }).filter(|probability| *probability >= 60.0);
            match (yesterday_dry, rain_probability) {
                (true, Some(probability)) => Some(("降雨", (probability.round() as i32), probability, today)),
                _ => None,
            }
        } else { None };
        let Some((kind, drop, expected, target_day)) = temperature_event.or(rain_event) else { return; };
        let target_date = daily.time.get(target_day).cloned().unwrap_or_default();
        let weekly_start = crate::model::next_local_date(-6);
        let created_at = crate::model::today_local();
        let mut changed = false;
        for (index, contact) in self.profile.care_contacts.iter().enumerate()
            .filter(|(_, c)| c.city.lat == city.lat && c.city.lon == city.lon)
        {
            let key = format!("{index}:{:.3}:{:.3}:{kind}:{target_date}", city.lat, city.lon);
            if self.profile.care_reminders.iter().any(|r| r.event_key == key) { continue; }
            let recent = self.profile.care_reminders.iter().filter(|r| {
                r.contact_index == index && r.created_at.as_str() >= weekly_start.as_str()
                    && r.created_at.as_str() <= created_at.as_str()
            }).count();
            if recent >= 2 { continue; }
            let fact = if kind == "降雨" {
                format!("{}未来 12 小时降雨概率最高约 {:.0}%，昨天无雨", contact.city.name, expected)
            } else {
                format!("{}明天{}约 {:.0}°C，较今天下降 {}°C", contact.city.name, kind, expected, drop)
            };
            let greeting = if contact.name.trim().is_empty() { String::new() } else { format!("{}，", contact.name.trim()) };
            let personal = if contact.personal_details.trim().is_empty() { String::new() } else { format!("{}。", contact.personal_details.trim()) };
            let close = match (kind, contact.tone.as_str()) {
                ("降雨", "轻松") => "下班路上记得带伞哈。",
                ("降雨", "简短") => "记得带伞。",
                ("降雨", _) => "出门记得带伞。",
                ("最高温", "轻松") => "明天出门多带件外套哈。",
                ("最高温", "简短") => "明天记得加衣。",
                ("最高温", _) => "明天出门记得加件衣服。",
                (_, "轻松") => "明天记得带伞呀。",
                (_, "简短") => "明天记得带伞。",
                _ => "明天出门前记得带伞。",
            };
            let draft = if kind == "降雨" {
                format!("{greeting}{}未来 12 小时降雨概率最高约 {:.0}%。{personal}{close}", contact.city.name, expected)
            } else {
                format!("{greeting}{}{}约 {:.0}°C，较今天下降 {}°C。{personal}{close}", contact.city.name, kind, expected, drop)
            };
            self.profile.care_reminders.push(crate::mine::CareReminder {
                contact_index: index, event_key: key, date: target_date.clone(), created_at: created_at.clone(), fact, draft, sent: false,
            });
            changed = true;
        }
        if changed {
            self.save_profile(cx);
            let profile = self.profile.clone();
            let results = self.wx.place_results.clone();
            let status = self.wx.place_status.clone();
            self.page(cx, NavHit::Me).borrow_mut::<MineScreen>()
                .map(|mut s| s.render_profile(cx, &profile, &results, &status));
        }
    }

    fn on_profile_storage(&mut self, cx: &mut Cx, responses: &[StorageResponse]) {
        for response in responses {
            if Some(response.request_id) != self.profile_load { continue; }
            self.profile_load = None;
            let Ok(StorageResult::Value(Some(bytes))) = &response.result else { continue; };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else { continue; };
            self.profile.wardrobe = value.get("wardrobe").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            self.profile.factors = value.get("factors").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
            self.profile.family_cities = value.get("family_cities").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let en = v.get("en").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let lat = v.get("lat")?.as_f64()?; let lon = v.get("lon")?.as_f64()?;
                Some(crate::model::City::custom(name, en, lat, lon))
            }).collect()).unwrap_or_default();
            self.profile.care_contacts = value.get("care_contacts").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|v| {
                let city = v.get("city")?;
                let name = city.get("name")?.as_str()?.to_string();
                let en = city.get("en").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let lat = city.get("lat")?.as_f64()?;
                let lon = city.get("lon")?.as_f64()?;
                Some(crate::mine::CareContact {
                    city: crate::model::City::custom(name, en, lat, lon),
                    name: v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    relationship: v.get("relationship").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    matrix_room: v.get("matrix_room").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    personal_details: v.get("personal_details").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    tone: v.get("tone").and_then(|x| x.as_str()).unwrap_or("叮嘱").to_string(),
                })
            }).collect()).unwrap_or_default();
            self.profile.care_reminders = value.get("care_reminders").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|v| {
                Some(crate::mine::CareReminder {
                    contact_index: v.get("contact_index")?.as_u64()? as usize,
                    event_key: v.get("event_key")?.as_str()?.to_string(),
                    date: v.get("date")?.as_str()?.to_string(),
                    created_at: v.get("created_at").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    fact: v.get("fact")?.as_str()?.to_string(),
                    draft: v.get("draft")?.as_str()?.to_string(),
                    sent: v.get("sent").and_then(|x| x.as_bool()).unwrap_or(false),
                })
            }).collect()).unwrap_or_default();
            // Migrate the earlier city-only setting without discarding it.
            for city in self.profile.family_cities.iter() {
                if !self.profile.care_contacts.iter().any(|c| c.city.lat == city.lat && c.city.lon == city.lon) {
                    self.profile.care_contacts.push(crate::mine::CareContact {
                        city: city.clone(), name: "家人".into(), relationship: "家人".into(),
                        matrix_room: String::new(), personal_details: String::new(), tone: "叮嘱".into(),
                    });
                }
            }
            if self.care_poll.is_empty() { self.care_poll = cx.start_interval(1800.0); }
            self.refresh_care_weather(cx);
            let schedule = self.schedule.clone(); let profile = self.profile.clone();
            let results = self.wx.place_results.clone(); let status = self.wx.place_status.clone();
            self.page(cx, NavHit::Me).borrow_mut::<MineScreen>().map(|mut s| { s.render(cx, &schedule); s.render_profile(cx, &profile, &results, &status); });
            let wx = self.wx.clone();
            self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                .map(|mut s| s.render(cx, &wx, &profile));
        }
    }
    fn page(&self, cx: &mut Cx, tab: NavHit) -> WidgetRef {
        match tab {
            NavHit::Weather => self.view.widget(cx, ids!(page_weather)),
            NavHit::Sky => self.view.widget(cx, ids!(page_gallery)),
            NavHit::Console => self.view.widget(cx, ids!(page_console)),
            NavHit::Me => self.view.widget(cx, ids!(page_me)),
        }
    }

    fn picker(&self, cx: &mut Cx) -> WidgetRef {
        self.view.widget(cx, ids!(page_picker))
    }

    fn nav(&self, cx: &mut Cx) -> WidgetRef {
        self.view.widget(cx, ids!(nav))
    }

    /// Attach the one assistant service the shell granted to this app scope.
    pub fn attach_assistant(&mut self, assistant: Option<Arc<dyn OctosAppService>>, scope: String) {
        self.assistant = assistant;
        self.instance_scope = scope;
        // 立刻绑账号：`set_account` 才会把 `ensure_peer` 丢到后台，助手才有机会
        // 在用户动手打字之前就绑好 peer。等第一次发消息再绑，徽章就永远停在
        // 「连接中」，而下面的拦截器又会拿那个标签挡掉这条消息——自锁死循环。
        if let Some(service) = self.assistant.as_ref() {
            service.set_account(Some(crate::agent::ACCOUNT));
        }
        // 重新算一次 badge 上的标签，可能之前显示的是「未连接」。
        self.refresh_model_label();
    }

    /// Host assistant client, created on the first user request.
    fn agent_of(&mut self) -> Result<&mut AgentClient, String> {
        if self.agent.is_none() {
            let service = self.assistant.clone().ok_or_else(|| {
                "AI 助手未连接：请在 OctoSense 主界面重新打开天气助手（要由主界面打开，\
                 独立启动的天气助手拿不到主界面的 AI 总线）"
                    .to_string()
            })?;
            self.agent = Some(AgentClient::new(service, &self.instance_scope)?);
            self.refresh_model_label();
        }
        Ok(self.agent.as_mut().unwrap())
    }

    /// 对话是否在跑。
    fn busy(&self) -> bool {
        self.agent.as_ref().is_some_and(|a| a.busy())
    }

    /// 把助手状态折成能贴到徽章上的字符串。判据是 broker 自己的
    /// `availability()`，**不是**「读没读到模型名」——内核的 `peer/prepare`
    /// 现在只回 `{"lane":"primary"}`，model 永远为空，拿它当「未配置」的依据
    /// 会把一切正常也报成未配置。见 `crate::agent::assistant_label`。
    /// 已有 agent 时沿用它的标签，保证和这一轮真正用的状态一致。
    fn refresh_model_label(&mut self) {
        let label = if let Some(agent) = self.agent.as_ref() {
            agent.model_label.clone()
        } else if let Some(svc) = self.assistant.as_ref() {
            crate::agent::assistant_label(svc.as_ref())
        } else {
            "AI 助手未连接".into()
        };
        self.model_label_cache = label;
    }

    fn model_label(&self) -> &str {
        &self.model_label_cache
    }

    /// 打开地区切换覆盖层：picker 亮、nav 藏、底色归零。
    fn open_picker(&mut self, cx: &mut Cx) {
        if self.picker_open {
            return;
        }
        self.picker_open = true;
        self.picker(cx).set_visible(cx, true);
        self.nav(cx).set_visible(cx, false);
        let wx = self.wx.clone();
        self.picker(cx).borrow_mut::<PlacePickerScreen>()
            .map(|mut s| s.render(cx, &wx));
    }

    /// 关掉覆盖层。
    fn close_picker(&mut self, cx: &mut Cx) {
        if !self.picker_open {
            return;
        }
        self.picker_open = false;
        self.picker(cx).set_visible(cx, false);
        self.nav(cx).set_visible(cx, true);
        let wx = self.wx.clone();
        let profile = self.profile.clone();
        self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
            .map(|mut s| s.render(cx, &wx, &profile));
    }

    /// AI 总线工具 `weather_agent.current` 的回答：当前屏幕上那份预报的文本摘要。
    /// 独立窗格和托管模块共用它，两条路拿到的都是同一份数据。
    pub fn weather_summary(&self) -> String {
        let status_ok = !matches!(self.wx.status, WxStatus::Error(_));
        crate::model::ai_summary(self.wx.city(), &self.wx.forecast, status_ok)
    }

    /// AI 总线工具 `weather_agent.schedule` 的回答：日程页正在显示的那几条。
    pub fn schedule_summary(&self) -> String {
        let items = self.schedule.visible();
        if items.is_empty() {
            return "日程页现在是空的。".to_string();
        }
        use std::fmt::Write;
        let mut s = String::new();
        let _ = write!(s, "日程页正在显示 {} 条：\n", items.len());
        for item in &items {
            let _ = write!(s, "  {} {} {}，", item.date, item.time, item.title);
            if !item.place.is_empty() {
                let _ = write!(s, "地点 {}，", item.place);
            }
            let _ = write!(s, "{}", if item.done { "已完成" } else { "未完成" });
            s.push('\n');
        }
        s
    }

    /// 开屏先取一次数，不然首屏停在「还没取数」，得手动点刷新才有数据。
    pub fn start_fetch(&mut self, cx: &mut Cx) {
        self.wx.start(cx);
    }

    /// 离窗时收尾：在途的取数取消，跑着的回答也停下来。
    /// 这个 widget 自己不再持有别的会活过 isolate 的东西。
    pub fn shutdown(&mut self, cx: &mut Cx) {
        if let Some((id, _)) = self.wx.flight.take() {
            cx.cancel_http_request(id);
        }
        if let Some(agent) = self.agent.as_mut() {
            agent.cancel();
        }
    }

    /// 刷三页 + 导航栏选中态 + 覆盖层。
    pub fn render_all(&mut self, cx: &mut Cx) {
        let nav = self.nav(cx);
        let busy = self.busy();
        let wx = self.wx.clone();
        let schedule = self.schedule.clone();
        let profile = self.profile.clone();
        nav.borrow::<NavBar>().map(|n| n.paint(cx, self.tab));
        self.page(cx, NavHit::Weather)
            .borrow_mut::<TodayScreen>()
            .map(|mut s| s.render(cx, &wx, &profile));
        self.page(cx, NavHit::Sky)
            .borrow_mut::<SkyGallery>()
            .map(|mut s| s.render(cx, self.wx.city(), &self.wx.forecast));
        self.page(cx, NavHit::Console)
            .borrow_mut::<ConsoleScreen>()
            .map(|mut s| s.render(cx, &self.chat, busy, self.model_label()));
        self.page(cx, NavHit::Me)
            .borrow_mut::<MineScreen>()
            .map(|mut s| { s.render(cx, &schedule); s.render_profile(cx, &self.profile, &self.wx.place_results, &self.wx.place_status); });
        if self.picker_open {
            self.picker(cx).borrow_mut::<PlacePickerScreen>()
                .map(|mut s| s.render(cx, &wx));
        }
    }

    /// 切页签：只切显隐，三页的状态都还在。
    fn set_tab(&mut self, cx: &mut Cx, tab: NavHit) {
        let nav = self.nav(cx);
        self.tab = tab;
        for hit in NavHit::ALL {
            self.page(cx, hit).set_visible(cx, hit == tab);
        }
        nav.borrow::<NavBar>().map(|n| n.paint(cx, tab));
        if tab == NavHit::Weather {
            let wx = self.wx.clone();
            let profile = self.profile.clone();
            self.page(cx, tab).borrow_mut::<TodayScreen>()
                .map(|mut screen| screen.render(cx, &wx, &profile));
        } else if tab == NavHit::Console {
            let busy = self.busy();
            let model = self.model_label().to_string();
            self.page(cx, tab).borrow_mut::<ConsoleScreen>()
                .map(|mut screen| screen.render(cx, &self.chat, busy, &model));
        }
    }

    /// 消化 agent 事件：把流式文本并进对话，工具调用就地答掉。
    fn pump_agent(&mut self, cx: &mut Cx, _event: &Event) {
        let console_page = self.page(cx, NavHit::Console);
        let agent = &mut self.agent;
        let chat = &mut self.chat;

        let Some(client) = agent.as_mut() else {
            return;
        };
        let events = client.drain();
        // 模型标签可能在前一轮里刚被刷新（agent_of()），同步过来。
        self.model_label_cache = client.model_label.clone();
        let model_label = client.model_label.clone();

        let mut changed = false;
        for ev in events {
            match ev {
                AgentUpdate::Progress(text) => {
                    if !text.trim().is_empty() {
                        chat.set_stream(&text);
                        changed = true;
                    }
                }
                AgentUpdate::Complete(text) => {
                    chat.set_stream(&text);
                    chat.finish_stream();
                    changed = true;
                }
                AgentUpdate::Failed(error) => {
                    chat.fail(&friendly_error(&error));
                    changed = true;
                }
            }
        }
        if changed {
            let busy = client.busy();
            console_page
                .borrow_mut::<ConsoleScreen>()
                .map(|mut s| s.render(cx, chat, busy, &model_label));
        }
        if !client.busy() && !self.agent_poll.is_empty() {
            cx.stop_timer(self.agent_poll);
            self.agent_poll = Timer::empty();
        }
    }

    fn pump_care_agent(&mut self, cx: &mut Cx, event: &Event) {
        let should_poll = match event {
            Event::Timer(timer_event) => self.care_ai_poll.is_timer(timer_event).is_some(),
            Event::Signal => true,
            _ => false,
        };
        if !should_poll { return; }
        let updates = self.care_agent.as_mut().map(AgentClient::drain).unwrap_or_default();
        for update in updates {
            match update {
                AgentUpdate::Progress(_) => {}
                AgentUpdate::Complete(raw) => {
                    if !self.care_ai_poll.is_empty() { cx.stop_timer(self.care_ai_poll); }
                    self.care_ai_poll = Timer::empty();
                    let target = self.care_ai_target.take();
                    let body = serde_json::from_str::<serde_json::Value>(&raw).ok()
                        .and_then(|value| value.get("body").and_then(|body| body.as_str()).map(str::to_string))
                        .map(|body| body.trim().to_string())
                        .filter(|body| !body.is_empty());
                    if let (Some((index, fallback)), Some(body)) = (target, body) {
                        let has_grounding = fallback.split(|c: char| !c.is_ascii_digit())
                            .filter(|part| !part.is_empty())
                            .any(|number| body.contains(number));
                        if !has_grounding {
                            self.wx.place_status = "AI 草稿没有保留天气数字，已保留事实版草稿，请检查后发送。".into();
                        } else {
                            let page = self.page(cx, NavHit::Me);
                            let mut replaced = false;
                            page.borrow_mut::<MineScreen>().map(|screen| {
                                if screen.care_edit_index() == Some(index)
                                    && screen.care_message(cx).text().trim() == fallback.trim()
                                {
                                    screen.care_message(cx).set_text(cx, &body);
                                    replaced = true;
                                }
                            });
                            self.wx.place_status = if replaced {
                                "AI 已按天气事实、关系和语气润色草稿；请检查和修改后再发送。".into()
                            } else {
                                "AI 草稿已完成；你已修改或切换联系人，因此没有覆盖当前内容。".into()
                            };
                        }
                    } else {
                        self.wx.place_status = "AI 返回格式不完整，已保留事实版草稿。".into();
                    }
                    self.render_care_status(cx);
                }
                AgentUpdate::Failed(error) => {
                    if !self.care_ai_poll.is_empty() { cx.stop_timer(self.care_ai_poll); }
                    self.care_ai_poll = Timer::empty();
                    self.care_ai_target = None;
                    self.wx.place_status = format!("AI 起草失败，事实版草稿仍可编辑和发送：{}", friendly_error(&error));
                    self.render_care_status(cx);
                }
            }
        }
    }

    fn render_care_status(&mut self, cx: &mut Cx) {
        let profile = self.profile.clone();
        let results = self.wx.place_results.clone();
        let status = self.wx.place_status.clone();
        self.page(cx, NavHit::Me).borrow_mut::<MineScreen>()
            .map(|mut screen| screen.render_profile(cx, &profile, &results, &status));
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        // picker 是覆盖层：覆盖天气页时，所有动作先交给 picker。
        if self.picker_open {
            self.handle_picker(cx, actions);
            return;
        }
        // 底部导航优先：点了就只切页签。
        if let Some(hit) = self.nav(cx).borrow::<NavBar>().and_then(|n| n.hit(cx, actions)) {
            self.set_tab(cx, hit);
            return;
        }

        match self.tab {
            NavHit::Weather => self.handle_weather(cx, actions),
            NavHit::Sky => {}
            NavHit::Console => self.handle_console(cx, actions),
            NavHit::Me => self.handle_mine(cx, actions),
        }
    }

    fn handle_mine(&mut self, cx: &mut Cx, actions: &Actions) {
        let page = self.page(cx, NavHit::Me);
        let row_hit = page.borrow::<MineScreen>().and_then(|s| s.row_hit(cx, actions));
        if let Some(which) = row_hit {
            let profile = self.profile.clone(); let results = self.wx.place_results.clone(); let status = self.wx.place_status.clone();
            page.borrow_mut::<MineScreen>().map(|mut s| { s.open_editor(cx, which, &profile); s.render_profile(cx, &profile, &results, &status); });
            return;
        }
        if page.borrow::<MineScreen>().is_some_and(|s| s.close_hit(cx, actions)) {
            let profile = self.profile.clone();
            page.borrow_mut::<MineScreen>().map(|mut s| {
                s.close_editor(cx);
                s.render_profile(cx, &profile, &self.wx.place_results, &self.wx.place_status);
            });
            let wx = self.wx.clone();
            self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                .map(|mut screen| screen.render(cx, &wx, &profile));
            return;
        }
        let editor = page.borrow::<MineScreen>().map(|s| s.editor()).unwrap_or(0);
        if editor == 1 && page.borrow::<MineScreen>().is_some_and(|s| s.save_hit(cx, actions)) {
            self.profile.wardrobe = page.borrow::<MineScreen>().map(|s| s.wardrobe_input(cx).text()).unwrap_or_default().trim().to_string();
            self.save_profile(cx);
            page.borrow_mut::<MineScreen>().map(|mut s| { s.close_editor(cx); s.render_profile(cx, &self.profile, &self.wx.place_results, &self.wx.place_status); });
            return;
        }
        if editor == 2 {
            if page.borrow::<MineScreen>().is_some_and(|s| s.custom_factor_plus_hit(cx, actions)) {
                page.borrow_mut::<MineScreen>().map(|mut s| s.toggle_custom_factor_entry(cx));
                return;
            }
            if let Some(i) = page.borrow::<MineScreen>().and_then(|s| s.custom_factor_hit(cx, actions)) {
                let factor = self.profile.factors.iter()
                    .filter(|value| !crate::mine::PREF_FACTORS.contains(&value.as_str()))
                    .nth(i).cloned();
                if let Some(factor) = factor {
                    self.profile.factors.retain(|value| value != &factor);
                    self.save_profile(cx);
                    let profile = self.profile.clone();
                    page.borrow_mut::<MineScreen>().map(|mut screen| {
                        screen.render_factors(cx, &profile);
                        screen.render_profile(cx, &profile, &self.wx.place_results, &self.wx.place_status);
                    });
                    let wx = self.wx.clone();
                    self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                        .map(|mut screen| screen.render(cx, &wx, &profile));
                }
                return;
            }
            if page.borrow::<MineScreen>().is_some_and(|s| s.add_custom_factor_hit(cx, actions)) {
                let label = page.borrow::<MineScreen>()
                    .map(|s| s.custom_factor_input(cx).text().trim().to_string()).unwrap_or_default();
                let custom_count = self.profile.factors.iter()
                    .filter(|value| !crate::mine::PREF_FACTORS.contains(&value.as_str())).count();
                let message = if label.is_empty() {
                    "先输入一个关注标签。"
                } else if label.chars().count() > 24 {
                    "标签最多 24 个字。"
                } else if self.profile.factors.iter().any(|value| value.eq_ignore_ascii_case(&label)) {
                    "这个标签已经添加了。"
                } else if custom_count >= 5 {
                    "最多添加 5 个自定义标签；点标签即可移除。"
                } else {
                    self.profile.factors.push(label);
                    self.save_profile(cx);
                    page.borrow::<MineScreen>().map(|s| s.custom_factor_input(cx).set_text(cx, ""));
                    page.borrow_mut::<MineScreen>().map(|mut s| s.hide_custom_factor_entry(cx));
                    "标签已保存；点标签可以移除。"
                };
                let profile = self.profile.clone();
                page.borrow_mut::<MineScreen>().map(|mut screen| {
                    screen.render_factors(cx, &profile);
                    screen.set_custom_factor_status(cx, message);
                    screen.render_profile(cx, &profile, &self.wx.place_results, &self.wx.place_status);
                });
                let wx = self.wx.clone();
                self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                    .map(|mut screen| screen.render(cx, &wx, &profile));
                return;
            }
            if let Some(i) = page.borrow::<MineScreen>().and_then(|s| s.factor_hit(cx, actions)) {
                let Some(factor) = crate::mine::PREF_FACTORS.get(i).copied() else { return; };
                if self.profile.factors.iter().any(|f| f == factor) { self.profile.factors.retain(|f| f != factor); } else { self.profile.factors.push(factor.into()); }
                self.save_profile(cx);
                let profile = self.profile.clone();
                page.borrow_mut::<MineScreen>().map(|mut s| {
                    s.render_factors(cx, &profile);
                    s.render_profile(cx, &profile, &self.wx.place_results, &self.wx.place_status);
                });
                let wx = self.wx.clone();
                self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                    .map(|mut screen| screen.render(cx, &wx, &profile));
                return;
            }
        }
        if editor == 3 {
            if page.borrow::<MineScreen>().is_some_and(|s| s.care_tone_hit(cx, actions)) {
                page.borrow_mut::<MineScreen>().map(|mut s| s.cycle_care_tone(cx));
                return;
            }
            if page.borrow::<MineScreen>().is_some_and(|s| s.care_load_rooms_hit(cx, actions)) {
                cx.widget_action(self.widget_uid(), crate::care_bridge::CareBridgeRequest::ListRooms);
                self.wx.place_status = "正在读取 Rinx 已加入的会话…".into();
                return;
            }
            if let Some(i) = page.borrow::<MineScreen>().and_then(|s| s.care_room_hit(cx, actions)) {
                if let Some(room) = page.borrow::<MineScreen>().and_then(|s| s.selected_care_room(i).map(str::to_string)) {
                    page.borrow_mut::<MineScreen>().map(|s| s.care_room(cx).set_text(cx, &room));
                }
                return;
            }
            if page.borrow::<MineScreen>().is_some_and(|s| s.care_alert_hit(cx, actions)) {
                let profile = self.profile.clone();
                let applied = page.borrow::<MineScreen>().is_some_and(|s| s.apply_care_alert(cx, &profile));
                self.wx.place_status = if applied { "已将天气提醒写入可编辑草稿，请检查后再发送。" } else { "目前没有待处理的天气提醒。" }.into();
                let results = self.wx.place_results.clone();
                let status = self.wx.place_status.clone();
                page.borrow_mut::<MineScreen>().map(|mut s| s.render_profile(cx, &profile, &results, &status));
                return;
            }
            if page.borrow::<MineScreen>().is_some_and(|s| s.care_draft_hit(cx, actions)) {
                let index = page.borrow::<MineScreen>().and_then(|s| s.care_edit_index());
                let input = index.and_then(|i| self.profile.care_contacts.get(i)).and_then(|contact| {
                    let city = self.wx.city();
                    if city.lat != contact.city.lat || city.lon != contact.city.lon { return None; }
                    self.wx.forecast.as_ref().map(|forecast| (contact.clone(), forecast.clone(), care_weather_draft(contact, forecast)))
                });
                if let Some((contact, forecast, fallback)) = input {
                    page.borrow_mut::<MineScreen>().map(|s| s.care_message(cx).set_text(cx, &fallback));
                    self.care_ai_target = Some((index.unwrap_or_default(), fallback.clone()));
                    if let Some(agent) = self.care_agent.as_mut() { agent.cancel(); }
                    let assistant = self.assistant.clone();
                    let ai_result = assistant.and_then(|assistant| {
                        let mut agent = AgentClient::new(assistant, &self.instance_scope).ok()?;
                        let profile = format!("关系：{}；称呼：{}；语气：{}；对方主动填写的生活细节：{}。这些资料只用于这一条草稿。",
                            contact.relationship, contact.name, contact.tone, contact.personal_details);
                        let prompt = format!("请为应用内聊天起草一条可直接发送、自然像家人朋友说话的中文关心消息。\n\
                             必须保留这条事实版草稿中的至少一个阿拉伯数字天气数值，不能编造、改写或补充天气事实。\n\
                             不要写标题、解释、提醒标签、引号或第二条备选文案；只写一条消息正文。\n\
                             语气按资料选择，关系和生活细节可自然融入，但不要强行使用。\n\
                             事实版草稿：{}", fallback);
                        agent.ask(&prompt, &contact.city, &Some(forecast), true, &profile).ok()?;
                        Some(agent)
                    });
                    if let Some(agent) = ai_result {
                        self.care_agent = Some(agent);
                        if !self.care_ai_poll.is_empty() { cx.stop_timer(self.care_ai_poll); }
                        self.care_ai_poll = cx.start_interval(0.1);
                        self.wx.place_status = "已先填入天气事实草稿，AI 正在按关系和语气润色；你可以继续编辑。".into();
                    } else {
                        self.care_ai_target = None;
                        self.wx.place_status = "AI 助手当前不可用，已生成天气事实版草稿；你可以编辑后发送。".into();
                    }
                } else {
                    self.wx.place_status = "先点“查看天气”加载这位联系人城市的预报，再生成有事实依据的草稿。".into();
                }
                let profile = self.profile.clone();
                let results = self.wx.place_results.clone();
                let status = self.wx.place_status.clone();
                page.borrow_mut::<MineScreen>().map(|mut s| s.render_profile(cx, &profile, &results, &status));
                return;
            }
            if page.borrow::<MineScreen>().is_some_and(|s| s.care_send_hit(cx, actions)) {
                let index = page.borrow::<MineScreen>().and_then(|s| s.care_edit_index());
                let room = index.and_then(|i| self.profile.care_contacts.get(i)).map(|c| c.matrix_room.trim().to_string()).unwrap_or_default();
                let text = page.borrow::<MineScreen>().map(|s| s.care_message(cx).text()).unwrap_or_default().trim().to_string();
                let event_key = index.and_then(|i| self.profile.care_reminders.iter().find(|r| r.contact_index == i && !r.sent).map(|r| r.event_key.clone()));
                if room.is_empty() || text.is_empty() {
                    self.wx.place_status = "请先填写 Rinx 会话名称和消息内容，再发送。".into();
                } else {
                    cx.widget_action(self.widget_uid(), crate::care_bridge::CareBridgeRequest::SendMessage { room, text, event_key });
                    self.wx.place_status = "已交给 Rinx 校验会话；发送前仍需你在聊天中确认。".into();
                }
                let profile = self.profile.clone();
                let results = self.wx.place_results.clone();
                let status = self.wx.place_status.clone();
                page.borrow_mut::<MineScreen>().map(|mut s| s.render_profile(cx, &profile, &results, &status));
                return;
            }
            if page.borrow::<MineScreen>().is_some_and(|s| s.care_save_hit(cx, actions)) {
                if let Some(index) = page.borrow::<MineScreen>().and_then(|s| s.care_edit_index()) {
                    if let Some(contact) = self.profile.care_contacts.get_mut(index) {
                        contact.name = page.borrow::<MineScreen>().map(|s| s.care_name(cx).text()).unwrap_or_default().trim().to_string();
                        contact.relationship = page.borrow::<MineScreen>().map(|s| s.care_relation(cx).text()).unwrap_or_default().trim().to_string();
                        contact.matrix_room = page.borrow::<MineScreen>().map(|s| s.care_room(cx).text()).unwrap_or_default().trim().to_string();
                        contact.personal_details = page.borrow::<MineScreen>().map(|s| s.care_details(cx).text()).unwrap_or_default().trim().to_string();
                        contact.tone = page.borrow::<MineScreen>().map(|s| s.care_tone().to_string()).unwrap_or_else(|| default_care_tone(&contact.relationship).into());
                    }
                    self.save_profile(cx);
                    let profile = self.profile.clone();
                    let results = self.wx.place_results.clone();
                    let status = self.wx.place_status.clone();
                    page.borrow_mut::<MineScreen>().map(|mut s| s.render_profile(cx, &profile, &results, &status));
                }
                return;
            }
            if let Some(i) = page.borrow::<MineScreen>().and_then(|s| s.edit_city_hit(cx, actions)) {
                let profile = self.profile.clone();
                page.borrow_mut::<MineScreen>().map(|mut s| s.open_care_fields(cx, i, &profile));
                return;
            }
            if let Some(i) = page.borrow::<MineScreen>().and_then(|s| s.saved_city_hit(cx, actions)) {
                if let Some(city) = self.profile.family_cities.get(i).cloned() {
                    self.wx.select_location(city);
                    self.wx.start(cx);
                    page.borrow_mut::<MineScreen>().map(|mut s| s.close_editor(cx));
                    self.set_tab(cx, NavHit::Weather);
                    return;
                }
            }
            let changed = actions.iter().find_map(|a| match a.as_widget_action().cast() {
                TextInputAction::Changed(t) | TextInputAction::Returned(t, _) => Some(t), _ => None
            });
            let clicked = page.borrow::<MineScreen>().is_some_and(|s| s.search_hit(cx, actions));
            if let Some(query) = changed {
                self.wx.search_city(cx, &query);
            } else if clicked {
                let query = page.borrow::<MineScreen>().map(|s| s.city_input(cx).text()).unwrap_or_default();
                self.wx.search_city(cx, &query);
            }
            if let Some(i) = page.borrow::<MineScreen>().and_then(|s| s.city_hit(cx, actions)) {
                if let Some(city) = self.wx.place_results.get(i).cloned() {
                    if !self.profile.family_cities.iter().any(|c| c.name == city.name && c.lat == city.lat && c.lon == city.lon) {
                        self.profile.family_cities.push(city.clone());
                        self.profile.care_contacts.push(crate::mine::CareContact {
                            city, name: "家人".into(), relationship: "家人".into(),
                            matrix_room: String::new(), personal_details: String::new(), tone: "叮嘱".into(),
                        });
                    }
                    self.save_profile(cx);
                    self.refresh_care_weather(cx);
                    self.wx.place_results.clear();
                    self.wx.place_status = "已添加家人所在城市。再次点开此卡片可搜索其他城市；在天气页选择城市查看实时预报。".into();
                }
            }
            let profile = self.profile.clone();
            let results = self.wx.place_results.clone();
            let status = self.wx.place_status.clone();
            page.borrow_mut::<MineScreen>().map(|mut s| s.render_profile(cx, &profile, &results, &status));
        }
    }

    /// 地区切换覆盖层上的动作：返回 / 选结果 / 提交搜索。
    fn handle_picker(&mut self, cx: &mut Cx, actions: &Actions) {
        let page = self.picker(cx);

        if page.borrow::<PlacePickerScreen>().is_some_and(|s| s.back_hit(cx, actions)) {
            self.close_picker(cx);
            return;
        }

        if let Some(i) = page.borrow::<PlacePickerScreen>().and_then(|s| s.place_hit(cx, actions)) {
            if let Some(place) = self.wx.place_results.get(i).cloned() {
                self.wx.select_location(place);
                self.wx.start(cx);
                self.close_picker(cx);
            }
            return;
        }

        // 三种触发：每次输入变化都重搜（中文单字符前缀友好），回车强制再搜一次，
        // 点搜索按钮也再搜一次。
        let changed_text: Option<String> = actions.iter().find_map(|action| {
            if let TextInputAction::Changed(text) = action.as_widget_action().cast() {
                Some(text)
            } else if let TextInputAction::Returned(text, _) = action.as_widget_action().cast() {
                Some(text)
            } else {
                None
            }
        });
        let search_clicked = page.borrow::<PlacePickerScreen>()
            .is_some_and(|s| s.search_hit(cx, actions));
        if let Some(text) = changed_text {
            self.wx.search_city(cx, &text);
            let wx = self.wx.clone();
            page.borrow_mut::<PlacePickerScreen>()
                .map(|mut s| s.render(cx, &wx));
        } else if search_clicked {
            let text = page.borrow::<PlacePickerScreen>()
                .map(|s| s.search_input(cx).text())
                .unwrap_or_default();
            self.wx.search_city(cx, &text);
            let wx = self.wx.clone();
            page.borrow_mut::<PlacePickerScreen>()
                .map(|mut s| s.render(cx, &wx));
        }
    }

    fn handle_weather(&mut self, cx: &mut Cx, actions: &Actions) {
        let page = self.page(cx, NavHit::Weather);

        if page.borrow::<TodayScreen>().is_some_and(|s| s.family_add_hit(cx, actions)) {
            self.set_tab(cx, NavHit::Me);
            let mine = self.page(cx, NavHit::Me);
            let profile = self.profile.clone();
            let results = self.wx.place_results.clone();
            let status = self.wx.place_status.clone();
            mine.borrow_mut::<MineScreen>().map(|mut s| {
                s.open_editor(cx, 3, &profile);
                s.render_profile(cx, &profile, &results, &status);
            });
            return;
        }

        if let Some(index) = page.borrow::<TodayScreen>().and_then(|s| s.scene_prompt_hit(cx, actions)) {
            let prompt = match index {
                0 => "明天适合晾被子吗？请结合当地湿度、降水和风，给我具体时间窗口。",
                1 => "今天几点跑步最合适？请结合气温、降水和空气状况给出时间建议。",
                2 => "周末露营会不会太冷或下雨？请结合预报给我准备建议。",
                _ => "今天适合洗车吗？请结合降水和未来几小时的天气给我建议。",
            };
            self.set_tab(cx, NavHit::Console);
            self.submit_question(cx, prompt);
            let busy = self.busy();
            let model = self.model_label().to_string();
            self.page(cx, NavHit::Console).borrow_mut::<ConsoleScreen>()
                .map(|mut s| s.render(cx, &self.chat, busy, &model));
            return;
        }

        // 地区入口打开搜索 picker。
        if page.borrow::<TodayScreen>().is_some_and(|s| s.switch_hit(cx, actions)) {
            self.open_picker(cx);
        }
    }

    fn handle_console(&mut self, cx: &mut Cx, actions: &Actions) {
        let page = self.page(cx, NavHit::Console);
        let mut changed = false;

        if let Some((entry_index, choice_index)) = page.borrow::<ConsoleScreen>()
            .and_then(|screen| screen.choice_hit(cx, actions, &self.chat))
        {
            if let Some((choice, approval, proposal)) = self.chat.apply_choice(entry_index, choice_index) {
                if approval {
                    let approved = matches!(choice.as_str(), "同意" | "允许" | "确认" | "添加");
                    if approved {
                        if let Some(proposal) = proposal {
                            let added = self.schedule.add_on(&proposal.title, &proposal.date, &proposal.time, &proposal.place);
                            self.chat.push_note(if added { "已将确认的日程加入课表。" } else { "日程信息无效，未写入课表；请检查日期和时间格式。" });
                            if added {
                                self.schedule.select_date(&proposal.date);
                                let schedule = self.schedule.clone();
                                self.page(cx, NavHit::Me).borrow_mut::<MineScreen>()
                                    .map(|mut s| s.render(cx, &schedule));
                            }
                        }
                    }
                    let busy = self.busy();
                    let model = self.model_label().to_string();
                    page.borrow_mut::<ConsoleScreen>().map(|mut s| s.render(cx, &self.chat, busy, &model));
                    return;
                }
                // A regular card choice is a normal follow-up turn.
                self.submit_question(cx, &choice);
                let busy = self.busy();
                let model = self.model_label().to_string();
                page.borrow_mut::<ConsoleScreen>().map(|mut s| s.render(cx, &self.chat, busy, &model));
                return;
            }
        }

        // 回车、发送按钮、快捷提问，都算发一条。
        let mut text: Option<String> = None;
        for action in actions {
            match action.as_widget_action().cast() {
                TextInputAction::Returned(t, _) => text = Some(t),
                _ => {}
            }
        }
        if let Some(i) = page.borrow::<ConsoleScreen>().and_then(|s| s.quick_hit(cx, actions)) {
            text = Some(console::quick_prompt(i));
        }

        let send = text.is_some() || page.borrow::<ConsoleScreen>().is_some_and(|s| s.send_hit(cx, actions));
        if send {
            let t = text.unwrap_or_else(|| {
                page.borrow::<ConsoleScreen>()
                    .map(|s| s.input(cx).text())
                    .unwrap_or_default()
            });
            let t = t.trim().to_string();
            if !t.is_empty() {
                self.submit_question(cx, &t);
            }
            page.borrow::<ConsoleScreen>()
                .map(|s| s.input(cx).set_text(cx, ""));
            changed = true;
        } else if page.borrow::<ConsoleScreen>().is_some_and(|s| s.stop_hit(cx, actions)) {
            if let Ok(agent) = self.agent_of() { agent.cancel(); }
            changed = true;
        }

        if changed {
            let busy = self.busy();
            let model = self.model_label().to_string();
            page.borrow_mut::<ConsoleScreen>().map(|mut s| s.render(cx, &self.chat, busy, &model));
        }
    }

    /// 把一条用户消息发给助手。
    ///
    /// **不要**拿徽章上的标签在这里拦消息：那个标签是显示用的，而它变成
    /// 「连接中 / 未配置」恰好说明助手还没绑好 peer——可绑定偏偏发生在下面
    /// `agent_of()` 里。拿标签拦消息等于「没绑好 → 不让你发 → 永远不绑」，
    /// 自锁死循环。该让消息发出去，让 broker 回真话：peer 还没绑好时
    /// `ensure_bound` 会等它绑完，真出错就带着原因走 `friendly_error`。
    fn submit_question(&mut self, cx: &mut Cx, text: &str) {
        self.chat.push_user(text);
        let city = self.wx.city().clone();
        let forecast = self.wx.forecast.clone();
        let status_ok = !matches!(self.wx.status, WxStatus::Error(_));
        if let Err(error) = self.try_ask(cx, text, &city, &forecast, status_ok) {
            self.chat.fail(&friendly_error(&error));
        }
    }

    fn try_ask(
        &mut self,
        cx: &mut Cx,
        text: &str,
        city: &crate::model::City,
        forecast: &Option<crate::model::Forecast>,
        status_ok: bool,
    ) -> Result<(), String> {
        let factors = if self.profile.factors.is_empty() { "未设置".to_string() } else { self.profile.factors.join("、") };
        let family = if self.profile.family_cities.is_empty() { "未设置".to_string() } else { self.profile.family_cities.iter().map(|c| c.name.as_ref()).collect::<Vec<_>>().join("、") };
        let wardrobe = if self.profile.wardrobe.is_empty() { "未设置" } else { &self.profile.wardrobe };
        let profile = format!("衣橱：{wardrobe}。天气关注因素：{factors}。家人所在城市：{family}。");
        let result = self.agent_of()?.ask(text, city, forecast, status_ok, &profile);
        if result.is_ok() && self.agent_poll.is_empty() {
            self.agent_poll = cx.start_interval(0.1);
        }
        result
    }

}

impl Widget for Shell {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.initial_console_rendered {
            self.initial_console_rendered = true;
            self.nav(cx).borrow::<NavBar>().map(|nav| nav.paint(cx, self.tab));
            if self.tab == NavHit::Console {
                let busy = self.busy();
                let model = self.model_label().to_string();
                self.page(cx, NavHit::Console).borrow_mut::<ConsoleScreen>()
                    .map(|mut screen| screen.render(cx, &self.chat, busy, &model));
            } else {
                let wx = self.wx.clone();
                let profile = self.profile.clone();
                self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                    .map(|mut screen| screen.render(cx, &wx, &profile));
            }
        }

        // 助手是在后台绑的（`set_account` 起的那个 `ensure_peer`），徽章得跟着
        // 它变，不然会一直挂在「连接中」。只在真的变了才重画控制台那页。
        let label_before = self.model_label_cache.clone();
        self.refresh_model_label();
        if label_before != self.model_label_cache {
            let busy = self.busy();
            let model = self.model_label().to_string();
            self.page(cx, NavHit::Console)
                .borrow_mut::<ConsoleScreen>()
                .map(|mut s| s.render(cx, &self.chat, busy, &model));
        }

        self.view.handle_event(cx, event, scope);

        if let Event::NetworkResponses(responses) = event {
            let mut changed = false;
            for response in responses {
                match response {
                    NetworkResponse::HttpResponse { request_id, response }
                        if self.wx.owns(*request_id) =>
                    {
                        self.wx.resolve(*request_id, Ok(response.clone()));
                        changed = true;
                    }
                    NetworkResponse::HttpResponse { request_id, response }
                        if self.wx.owns_place_request(*request_id) =>
                    {
                        self.wx.resolve_place(*request_id, Ok(response.clone()));
                        changed = true;
                    }
                    NetworkResponse::HttpError { request_id, error }
                        if self.wx.owns(*request_id) =>
                    {
                        self.wx.resolve(*request_id, Err(error.message.clone()));
                        changed = true;
                    }
                    NetworkResponse::HttpError { request_id, error }
                        if self.wx.owns_place_request(*request_id) =>
                    {
                        self.wx.resolve_place(*request_id, Err(error.message.clone()));
                        changed = true;
                    }
                    NetworkResponse::HttpResponse { request_id, response }
                        if self.care_flights.iter().any(|(id, _)| id == request_id) =>
                    {
                        self.resolve_care_weather(cx, *request_id, Ok(response.clone()));
                    }
                    NetworkResponse::HttpError { request_id, error }
                        if self.care_flights.iter().any(|(id, _)| id == request_id) =>
                    {
                        self.resolve_care_weather(cx, *request_id, Err(error.message.clone()));
                    }
                    _ => {}
                }
            }
            if let Some(city) = self.wx.take_located_city() {
                self.wx.select_location(city);
                self.wx.start(cx);
                changed = true;
            }
            if changed {
                let wx = self.wx.clone();
                let profile = self.profile.clone();
                if self.picker_open {
                    // picker 打开时，network 响应主要去刷新 picker 自己的状态。
                    self.picker(cx).borrow_mut::<PlacePickerScreen>()
                        .map(|mut s| s.render(cx, &wx));
                }
                self.page(cx, NavHit::Weather).borrow_mut::<TodayScreen>()
                    .map(|mut s| s.render(cx, &wx, &profile));
                self.page(cx, NavHit::Sky).borrow_mut::<SkyGallery>()
                    .map(|mut s| s.render(cx, self.wx.city(), &self.wx.forecast));
                let schedule = self.schedule.clone();
                self.page(cx, NavHit::Me).borrow_mut::<MineScreen>()
                    .map(|mut s| { s.render(cx, &schedule); s.render_profile(cx, &self.profile, &self.wx.place_results, &self.wx.place_status); });
            }
        }

        if let Event::Storage(responses) = event { self.on_profile_storage(cx, responses); }
        if let Event::Timer(timer_event) = event {
            if self.care_poll.is_timer(timer_event).is_some() { self.refresh_care_weather(cx); }
        }
        if let Event::Custom(raw) = event {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            if value.get("kind").and_then(|v| v.as_str()) == Some("care_rooms") {
                let rooms = value.get("rooms").and_then(|v| v.as_array()).map(|items| items.iter().filter_map(|item| {
                    Some((item.get("id")?.as_str()?.to_string(), item.get("name")?.as_str()?.to_string()))
                }).collect()).unwrap_or_default();
                let status = value.get("status").and_then(|v| v.as_str()).unwrap_or("没有可用会话");
                self.page(cx, NavHit::Me).borrow_mut::<MineScreen>()
                    .map(|mut s| s.set_care_rooms(cx, rooms, status));
            } else if value.get("kind").and_then(|v| v.as_str()) == Some("care_sent") {
                let key = value.get("event_key").and_then(|v| v.as_str()).unwrap_or_default();
                let sent = value.get("sent").and_then(|v| v.as_bool()).unwrap_or(false);
                if sent {
                    if let Some(reminder) = self.profile.care_reminders.iter_mut().find(|r| r.event_key == key) { reminder.sent = true; }
                    self.save_profile(cx);
                    self.wx.place_status = "Rinx 已确认 Matrix 发送成功；已读状态可在对应会话查看。".into();
                } else {
                    self.wx.place_status = value.get("message").and_then(|v| v.as_str()).unwrap_or("Rinx 未发送这条消息，提醒仍保留待处理状态。").to_string();
                }
                let profile = self.profile.clone();
                let results = self.wx.place_results.clone();
                let status = self.wx.place_status.clone();
                self.page(cx, NavHit::Me).borrow_mut::<MineScreen>()
                    .map(|mut s| s.render_profile(cx, &profile, &results, &status));
            }
            }
        }

        if let Event::Actions(actions) = event {
            if self.tab == NavHit::Console {
                let page = self.page(cx, NavHit::Console);
                let relevant_actions = actions.iter()
                    .filter_map(|action| action.as_widget_action())
                    .filter(|action| {
                        action.action.is::<ButtonAction>() || action.action.is::<TextInputAction>()
                    })
                    .map(|action| format!("uid{} {:?}", action.widget_uid.0, action.action))
                    .collect::<Vec<_>>();
                self.handle_actions(cx, actions);
                if !relevant_actions.is_empty() && self.busy() {
                    page.borrow_mut::<ConsoleScreen>().map(|screen| {
                        screen.show_event_debug(cx, "消息已发送 · 正在等待天气助手回复…")
                    });
                }
            } else {
                self.handle_actions(cx, actions);
            }
        }

        self.pump_agent(cx, event);
        self.pump_care_agent(cx, event);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

/// 把内核 / 总线原样回吐的错误归一化成本应用能直接贴到对话气泡上的话。
/// 不要在这里写密钥、token、keychain 之类的敏感字眼。
fn friendly_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("assistant connection ended")
        || lower.contains("kernel stopped")
        || lower.contains("kernel closed its output")
    {
        return "AI 助手连接中断：请在 OctoSense 主界面重启 AI 服务后再试".to_string();
    }
    if lower.contains("turn running") || lower.contains("already has") {
        return "上一条消息还在处理中，请等它答完再发".to_string();
    }
    if lower.contains("upcr-2026-034") || lower.contains("host-owned app peers") {
        return "OctoSense 内核版本过旧，不支持宿主代管的应用会话：请更新内核后再试".to_string();
    }
    if lower.contains("account changed") {
        return "AI 助手会话已失效：请重新打开天气应用再试".to_string();
    }
    let looks_unconfigured = lower.contains("no_provider")
        || lower.contains("no provider")
        || lower.contains("not configured")
        || lower.contains("unconfigured")
        || lower.contains("provider is none")
        || raw.contains("未配置")
        || raw.contains("没有配置");
    let looks_no_key = lower.contains("no key")
        || lower.contains("missing key")
        || lower.contains("invalid key")
        || lower.contains("api key")
        || raw.contains("密钥")
        || raw.contains("Key 不正确");
    let looks_no_service = raw.contains("系统助手未连接")
        || lower.contains("no assistant")
        || lower.contains("assistant service is gone")
        || lower.contains("assistant was closed")
        || lower.contains("assistant was gone")
        || lower.contains("the app was closed")
        || lower.contains("access was closed")
        || lower.contains("sign in")
        || lower.contains("not granted")
        || lower.contains("not authorized");
    let looks_timeout = lower.contains("timed out") || lower.contains("timeout");
    let looks_kb_interrupt = lower.contains("cancel") || lower.contains("interrupt");
    if looks_unconfigured {
        "AI 助手尚未配置：打开 AI Providers 应用，完成模型配置后再试".to_string()
    } else if looks_no_key {
        "AI 助手的 API 密钥缺失或失效：前往 AI Providers 重新填写并测试".to_string()
    } else if looks_no_service {
        "AI 助手未连接：请在 OctoSense 主界面启用 AI 助手，再重新打开天气应用".to_string()
    } else if looks_timeout {
        "AI 助手超时没答回来：请再发一次试试".to_string()
    } else if looks_kb_interrupt {
        "已停止这一轮".to_string()
    } else {
        raw.to_string()
    }
}

fn default_care_tone(relationship: &str) -> &'static str {
    match relationship.trim() {
        "妈妈" | "母亲" | "爸爸" | "父亲" | "爷爷" | "奶奶" | "外公" | "外婆" => "叮嘱",
        "伴侣" | "爱人" | "丈夫" | "妻子" => "轻松",
        _ => "简短",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_is_a_real_widget() {
        assert!(std::any::type_name::<Shell>().ends_with("Shell"));
    }

    #[test]
    fn friendly_error_maps_known_substrings() {
        assert!(friendly_error("no_provider").contains("AI Providers"));
        assert!(friendly_error("model not configured").contains("AI Providers"));
        assert!(friendly_error("invalid api key").contains("API 密钥"));
        assert!(friendly_error("assistant service is gone").contains("启用 AI 助手"));
        assert!(friendly_error("The app was closed").contains("启用 AI 助手"));
        assert!(friendly_error("Sign in to use the assistant").contains("启用 AI 助手"));
        assert!(friendly_error("user cancel").contains("已停止"));
        // 「已有消息在跑」不能因为话里带 assistant 就被误判成未连接。
        assert!(friendly_error("This app already has an assistant turn running").contains("还在处理中"));
        assert!(friendly_error("The account changed; reopen this app").contains("会话已失效"));
        assert!(friendly_error("octos UPCR-2026-034 not supported").contains("内核版本过旧"));
        assert!(friendly_error("The assistant's turn timed out").contains("超时"));
        assert!(friendly_error("The assistant connection ended (the octos kernel stopped)").contains("连接中断"));
        // 陌生错误原样返回。
        assert_eq!(friendly_error("segfault in the draw pass"), "segfault in the draw pass");
    }
}

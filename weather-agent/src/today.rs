//! The redesigned decision-first home screen.
use makepad_widgets::*;

use crate::model::{self, Forecast};
use crate::mine::WeatherProfile;
use crate::weather::{WeatherState, WxStatus};
use makepad_widgets::Vec4f;
use std::path::Path;
use std::sync::Arc;

const HOURS: [&str; 5] = ["h0", "h1", "h2", "h3", "h4"];
const BACKGROUND_CLEAR: &[u8] = include_bytes!("assets/weather-backgrounds/clear.png");
const BACKGROUND_CLOUDY: &[u8] = include_bytes!("assets/weather-backgrounds/cloudy.png");
const BACKGROUND_RAIN: &[u8] = include_bytes!("assets/weather-backgrounds/rain.png");
const BACKGROUND_SNOW: &[u8] = include_bytes!("assets/weather-backgrounds/snow.png");
const BACKGROUND_STORM: &[u8] = include_bytes!("assets/weather-backgrounds/storm.png");
const BACKGROUND_NIGHT: &[u8] = include_bytes!("assets/weather-backgrounds/night.png");

fn weather_background(code: Option<i32>, is_day: bool) -> (&'static str, &'static [u8]) {
    match code {
        Some(95..=99) => ("weather-agent/card-storm.png", BACKGROUND_STORM),
        Some(51..=67 | 80..=82) => ("weather-agent/card-rain.png", BACKGROUND_RAIN),
        Some(71..=77 | 85..=86) => ("weather-agent/card-snow.png", BACKGROUND_SNOW),
        Some(3 | 45 | 48) => ("weather-agent/card-cloudy.png", BACKGROUND_CLOUDY),
        Some(0 | 1 | 2) if !is_day => ("weather-agent/card-night.png", BACKGROUND_NIGHT),
        Some(0 | 1) => ("weather-agent/card-clear.png", BACKGROUND_CLEAR),
        Some(_) => ("weather-agent/card-cloudy.png", BACKGROUND_CLOUDY),
        None => ("weather-agent/card-cloudy.png", BACKGROUND_CLOUDY),
    }
}

fn weather_ink(code: i32) -> Vec4f {
    match code {
        0 => vec4(1.0, 0.81, 0.43, 1.0),
        1 | 2 => vec4(0.87, 0.93, 0.96, 1.0),
        3 | 45 | 48 => vec4(0.80, 0.85, 0.90, 1.0),
        51..=67 | 80..=82 => vec4(0.54, 0.82, 1.0, 1.0),
        71..=77 | 85..=86 => vec4(0.76, 0.91, 1.0, 1.0),
        95..=99 => vec4(0.87, 0.76, 1.0, 1.0),
        _ => vec4(0.73, 0.90, 0.82, 1.0),
    }
}

fn local_hour(time: Option<&str>) -> u32 {
    time.and_then(|value| value.split('T').nth(1))
        .and_then(|clock| clock.get(..2))
        .and_then(|hour| hour.parse().ok())
        .unwrap_or(12)
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let Muted = Label{
        width: Fill height: Fit padding: 0 margin: 0 max_lines: 2
        draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 10.0} }
    }
    let Head = Label{
        width: Fill height: Fit padding: 0 margin: 0
        draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 13.0} }
    }
    let Card = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 9
        margin: Inset{left: 14 right: 14}
        padding: Inset{top: 14 left: 15 right: 15 bottom: 14}
        show_bg: true
        draw_bg +: { color: #252525 border_radius: 14.0 border_size: 0.8 border_color: #383838 }
    }
    let PlaceAction = View{
        width: Fit height: 30 padding: Inset{top: 0 left: 10 right: 10 bottom: 0}
        align: Align{x: 0.5 y: 0.5} cursor: MouseCursor.Hand show_bg: true
        draw_bg +: { color: #3A3027 border_radius: 15.0 }
        title := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "换城市"
            draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 10.0} } }
    }
    let HeroCard = View{
        width: Fill height: 258 flow: Overlay
        show_bg: false
        background := Image{ width: Fill height: Fill fit: ImageFit.CropToFill visible: false }
        View{ width: Fill height: Fill show_bg: true draw_bg +: { color: #07111A48 } }
        View{ width: Fill height: Fill show_bg: true
            draw_bg +: {
                pixel: fn(){
                    let alpha = smoothstep(0.68, 1.0, self.pos.y)
                    return vec4(0.91 * alpha, 0.95 * alpha, 0.96 * alpha, alpha)
                }
            }
        }
        content := View{ width: Fill height: Fill flow: Down spacing: 10
            padding: Inset{top: 18 left: 20 right: 20 bottom: 18}
            View{ width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                View{ width: 30 height: 30 align: Align{x: 0.5 y: 0.5} show_bg: true
                    draw_bg +: { color: #493521C0 border_radius: 15.0 }
                    Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 0 margin: 0 text: "知"
                        draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 13.0} } }
                }
                View{ width: Fill height: Fit flow: Down spacing: 2
                    Head{ text: "知时" }
                    Muted{ text: "让天气为今天做决定" }
                }
                status_pill := View{ width: Fit height: Fit align: Align{x: 1.0} show_bg: true
                    padding: Inset{top: 5 left: 8 right: 8 bottom: 5}
                    draw_bg +: { color: #101C2AB0 border_radius: 10.0 }
                    weather_status := Muted{ width: Fit }
                }
            }
            View{ width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                city_button := View{ width: Fit height: 48 flow: Down spacing: 3 cursor: MouseCursor.Hand show_bg: true
                    align: Align{y: 0.5}
                    draw_bg +: { color: #00000000 }
                    city_line := View{width: Fit height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                        city := Head{ width: Fit text: "北京 · 中国" }
                        city_chevron := Label{width: Fit height: Fit padding: 0 margin: 0 text: "⌄"
                            draw_text +: {color: #F3A44D text_style: theme.font_bold{font_size: 12.0}}}
                    }
                    stamp := Muted{ text: "此刻天气" }
                }
                place_switch := PlaceAction{}
            }
            View{ width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                temp := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "—°"
                    draw_text +: { color: #FFFFFF text_style: theme.font_regular{font_size: 54.0} } }
                View{ width: Fill height: Fit flow: Down spacing: 5
                    condition := Head{ text: "天气更新中" }
                    high_low := Muted{ text: "最高 —° · 最低 —°" }
                }
                weather_icon := Label{ width: 54 height: 54 padding: 0 margin: 0 align: Align{x: 0.5 y: 0.5} text: "☁"
                    draw_text +: { color: #DCEAF4 text_style: theme.font_regular{font_size: 38.0} } }
            }
        }
    }
    let SmartCard = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 9
        margin: Inset{left: 14 right: 14}
        padding: Inset{top: 14 left: 16 right: 16 bottom: 14}
        show_bg: true
        draw_bg +: { color: #30291F border_radius: 14.0 border_size: 0.8 border_color: #6C5438 }
    }
    let Action = Button{width: Fit height: 36 padding: Inset{left: 12 right: 12}
        draw_bg +: {color: #5B503F color_hover: #71624D color_down: #493F32 border_radius: 18.0}
        draw_text +: {color: #F8EBD5 text_style: theme.font_regular{font_size: 11.0}}
    }
    let SelectedAction = Button{width: Fit height: 36 padding: Inset{left: 12 right: 12}
        draw_bg +: {color: #F2B96F color_hover: #FFC980 color_down: #DFA45A border_radius: 18.0}
        draw_text +: {color: #30251A text_style: theme.font_bold{font_size: 11.0}}
    }
    let Hour = View{
        width: Fill height: Fit flow: Down spacing: 6 align: Align{x: 0.5}
        at := Muted{ align: Align{x: 0.5} }
        icon := Label{ width: Fill height: 22 padding: 0 margin: 0 align: Align{x: 0.5 y: 0.5}
            draw_text +: { color: #F3A44D text_style: theme.font_regular{font_size: 17.0} } }
        temp := Label{ width: Fill height: Fit padding: 0 margin: 0 align: Align{x: 0.5}
            draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 11.0} } }
    }

    mod.widgets.TodayScreenBase = #(TodayScreen::register_widget(vm))
    mod.widgets.TodayScreen = set_type_default() do mod.widgets.TodayScreenBase{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: { color: #191919 }
        ScrollYView{
            width: Fill height: Fill flow: Down spacing: 10
            padding: Inset{top: 0 left: 0 right: 0 bottom: 14}
            show_bg: true
            draw_bg +: { color: #191919 }
            scroll_bars +: { show_scroll_x: false }

            hero := HeroCard{}

            family_card := Card{flow: Overlay
                cursor: MouseCursor.Hand
                padding: Inset{top: 16 left: 17 right: 17 bottom: 16}
                show_bg: true
                draw_bg +: { color: #30271D border_radius: 16.0 border_size: 1.0 border_color: #79572F }
                View{ width: Fill height: Fit flow: Right spacing: 11 align: Align{y: 0.5}
                    View{ width: 38 height: 38 align: Align{x: 0.5 y: 0.5} show_bg: true
                        draw_bg +: { color: #493521 border_radius: 19.0 }
                        Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 0 margin: 0 text: "家"
                            draw_text +: { color: #F2C98D text_style: theme.font_bold{font_size: 13.0} } }
                    }
                    View{ width: Fill height: Fit flow: Down spacing: 4
                        Head{ text: "家人天气提醒" }
                        family_summary := Muted{ text: "添加家人所在城市，开启降温与降雨提醒" }
                    }
                    family_cta := PlaceAction{title.text: "添加家人 +"}
                }
                family_add := ButtonFlat{width: Fill height: Fill text: "" padding: 0 margin: 0
                    draw_bg +: {color: uniform(#00000000) color_hover: uniform(#00000000) color_down: uniform(#00000000) color_focus: uniform(#00000000)
                        border_size: uniform(0.0) border_color: uniform(#00000000) border_color_hover: uniform(#00000000) border_color_down: uniform(#00000000) border_color_focus: uniform(#00000000)}
                    draw_text +: {color: #00000000 text_style: theme.font_regular{font_size: 1.0}}
                }
            }

            SmartCard{
                View{ width: Fill height: Fit flow: Right spacing: 7 align: Align{y: 0.5}
                    Label{ width: Fit height: Fit padding: 0 margin: 0 text: "AI"
                        draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 11.0} } }
                    Head{ text: "今日建议" }
                    View{ width: Fill height: 1 }
                    Muted{ width: Fit text: "按当地预报整理" }
                }
                morning := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
                    draw_text +: { color: #F6F2EA text_style: theme.font_regular{font_size: 11.5 line_spacing: 1.5} } }
            }

            SmartCard{
                View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    Head{ text: "现在想做什么？" }
                    View{width: Fill height: 1}
                    Muted{width: Fit text: "选场景看安排"}
                }
                View{ width: Fill height: Fit flow: Right spacing: 7
                    ask_dry := SelectedAction{text: "晾被子"}
                    ask_run := Action{text: "去跑步"}
                    ask_camp := Action{text: "露营"}
                    ask_wash := Action{text: "洗车"}
                }
                scene_answer := Label{width: Fill height: Fit padding: 0 margin: 0 max_lines: 2 text: "选择场景，查看天气适宜时段。"
                    draw_text +: { color: #F1E7D8 text_style: theme.font_regular{font_size: 11.0 line_spacing: 1.5} } }
            }

            View{ width: Fill height: Fit flow: Right spacing: 9
                Card{ width: Fill
                    Head{ text: "今日穿搭" }
                    outfit := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
                        draw_text +: { color: #E9E4DB text_style: theme.font_regular{font_size: 10.5 line_spacing: 1.4} } }
                    outfit_hint := Muted{ text: "示例建议 · 完善衣橱后可个性化" }
                }
                Card{ width: Fill
                    Head{ text: "身体气象站" }
                    bodycast := Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 3
                        draw_text +: { color: #E9E4DB text_style: theme.font_regular{font_size: 10.5 line_spacing: 1.4} } }
                    Muted{ text: "天气提示，不替代医疗建议" }
                }
            }

            Card{
                View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                    Head{ text: "接下来几小时" }
                    View{ width: Fill height: 1 }
                    Muted{ width: Fit text: "逐小时预报" }
                }
                View{ width: Fill height: Fit flow: Right spacing: 4
                    h0 := Hour{}
                    h1 := Hour{}
                    h2 := Hour{}
                    h3 := Hour{}
                    h4 := Hour{}
                }
            }
            Muted{ width: Fill margin: Inset{left: 14 right: 14} text: "天气数据来自 Open-Meteo · 城市时间" }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct TodayScreen {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust(String::new())]
    background_key: String,
}

impl TodayScreen {
    pub fn switch_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        let content = self.view.widget(cx, ids!(hero)).widget(cx, ids!(content));
        content.view(cx, ids!(city_button)).finger_up(actions).is_some()
            || content.view(cx, ids!(place_switch)).finger_up(actions).is_some()
    }

    pub fn scene_prompt_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for (i, id) in [ids!(ask_dry), ids!(ask_run), ids!(ask_camp), ids!(ask_wash)].into_iter().enumerate() {
            let b = self.view.button(cx, id);
            if b.pressed(actions) || b.clicked(actions) { return Some(i); }
        }
        None
    }

    pub fn family_add_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        let b = self.view.widget(cx, ids!(family_card)).button(cx, ids!(family_add));
        b.pressed(actions) || b.clicked(actions)
    }

    pub fn render(&mut self, cx: &mut Cx, state: &WeatherState, profile: &WeatherProfile) {
        let city = state.city();
        self.view.widget(cx, ids!(hero)).widget(cx, ids!(content)).widget(cx, ids!(city_button)).widget(cx, ids!(city_line)).label(cx, ids!(city))
            .set_text(cx, &format!("{} · {}", city.name, city.en));
        let status = match &state.status {
            WxStatus::Idle => "正在连接天气".to_string(),
            WxStatus::Loading => "更新天气中…".to_string(),
            WxStatus::Done => "实时预报".to_string(),
            WxStatus::Error(_) => "天气暂不可用".to_string(),
        };
        self.view.widget(cx, ids!(hero)).widget(cx, ids!(content)).widget(cx, ids!(status_pill))
            .label(cx, ids!(weather_status)).set_text(cx, &status);
        let family = self.view.widget(cx, ids!(family_card));
        if profile.family_cities.is_empty() {
            family.label(cx, ids!(family_summary)).set_text(cx, "添加家人所在城市，开启降温与降雨提醒；发送消息由你确认");
            family.widget(cx, ids!(family_cta)).label(cx, ids!(title)).set_text(cx, "添加家人 +");
        } else {
            let names = profile.family_cities.iter().map(|city| city.name.as_ref()).collect::<Vec<_>>().join("、");
            let pending = profile.care_reminders.iter().filter(|reminder| !reminder.sent).count();
            let summary = if pending > 0 {
                format!("待确认 {pending} 条提醒 · {names}")
            } else {
                format!("{} 位联系人正在监测降温与降雨 · {names}", profile.care_contacts.len())
            };
            family.label(cx, ids!(family_summary)).set_text(cx, &summary);
            family.widget(cx, ids!(family_cta)).label(cx, ids!(title)).set_text(cx, if pending > 0 { "查看提醒" } else { "管理家人" });
        }
        let Some(forecast) = state.forecast.as_ref() else {
            self.set_weather_background(cx, None, true);
            let content = self.view.widget(cx, ids!(hero)).widget(cx, ids!(content));
            content.widget(cx, ids!(city_button)).label(cx, ids!(stamp)).set_text(cx, &model::today_local());
            content.label(cx, ids!(condition)).set_text(cx, "正在获取当地天气…");
            self.view.label(cx, ids!(morning)).set_text(cx, "天气数据到达后，为你整理今天的穿衣和出行建议。");
            self.view.label(cx, ids!(scene_answer)).set_text(cx, "天气暂不可用，先不猜晾晒时段；联网后会结合降水和风更新建议。");
            if profile.wardrobe.trim().is_empty() {
                self.view.label(cx, ids!(outfit)).set_text(cx, "等待天气数据");
                self.view.label(cx, ids!(outfit_hint)).set_text(cx, "示例建议 · 完善衣橱后可个性化");
            } else {
                self.view.label(cx, ids!(outfit)).set_text(cx, &format!("你的衣橱：{}", profile.wardrobe));
                self.view.label(cx, ids!(outfit_hint)).set_text(cx, "已同步衣橱；天气数据到达后补充搭配建议");
            }
            let bodycast = if profile.factors.is_empty() {
                "等待天气数据".to_string()
            } else {
                format!("已关注：{}；天气数据到达后显示对应提示", profile.factors.join(" · "))
            };
            self.view.label(cx, ids!(bodycast)).set_text(cx, &bodycast);
            return;
        };
        self.paint_forecast(cx, city.name.as_ref(), forecast, profile);
    }

    fn paint_forecast(&mut self, cx: &mut Cx, city: &str, f: &Forecast, profile: &WeatherProfile) {
        let content = self.view.widget(cx, ids!(hero)).widget(cx, ids!(content));
        let current = f.current.as_ref();
        let code = current.and_then(|v| v.weather_code).unwrap_or(-1);
        let condition = model::condition(code);
        let temp = current.and_then(|v| v.temperature_2m).map(|v| format!("{v:.0}°")).unwrap_or_else(|| "—°".into());
        content.label(cx, ids!(temp)).set_text(cx, &temp);
        content.label(cx, ids!(condition)).set_text(cx, condition);
        let hour = local_hour(current.and_then(|v| v.time.as_deref()));
        let is_day = current.and_then(|v| v.is_day).map(|v| v != 0).unwrap_or((7..19).contains(&hour));
        self.set_weather_background(cx, current.and_then(|v| v.weather_code), is_day);
        let icon = if !is_day && matches!(code, 0 | 1 | 2) { "☾" } else { model::weather_icon(code) };
        content.label(cx, ids!(weather_icon)).set_text(cx, icon);
        let ink = weather_ink(code);
        crate::ui::tint(&content.label(cx, ids!(weather_icon)), cx, ink);
        crate::ui::tint(&content.label(cx, ids!(condition)), cx, ink);
        let stamp = current.and_then(|v| v.time.as_ref()).map(|t| format!("当地 · {}", model::hhmm(t))).unwrap_or_else(|| model::today_local());
        content.widget(cx, ids!(city_button)).label(cx, ids!(stamp)).set_text(cx, &stamp);

        let daily = f.daily.as_ref();
        let today_i = daily.and_then(|d| d.time.iter().position(|d| d == &model::today_local())).unwrap_or(0);
        let range = daily.and_then(|d| Some((d.temperature_2m_max.get(today_i)?, d.temperature_2m_min.get(today_i)?)))
            .map(|(hi, lo)| format!("最高 {:.0}° · 最低 {:.0}°", hi, lo)).unwrap_or_else(|| "最高 —° · 最低 —°".into());
        content.label(cx, ids!(high_low)).set_text(cx, &range);

        let wind = current.and_then(|v| v.wind_speed_10m).unwrap_or(0.0);
        let wet = current.and_then(|v| v.precipitation).unwrap_or(0.0) > 0.1 || matches!(code, 51..=67 | 80..=82 | 95..=99);
        let advice = if wet {
            format!("{city}现在{condition}，出门记得带伞。{}{}", range, if wind >= 25.0 { "，风也比较大，外套拉链拉好。" } else { "。" })
        } else if wind >= 25.0 {
            format!("{city}现在{condition}，风有些大，外套拉链记得拉好。{range}。")
        } else {
            format!("{city}现在{condition}，适合轻装出门。{range}，早晚温差记得留意。")
        };
        self.view.label(cx, ids!(morning)).set_text(cx, &advice);
        let scene_answer = if wet {
            "晾被子先等等：当前有降水，建议等雨停、湿度下降后再安排。"
        } else if wind >= 25.0 {
            "晾被子可选午前的短时晴窗；风偏大，记得固定好衣物。"
        } else if current.and_then(|v| v.relative_humidity_2m).unwrap_or(50.0) >= 78.0 {
            "晾被子建议选日照较好的中午时段；空气偏潮，留意回收时间。"
        } else {
            "适合安排晾晒：优先选上午到午后，日照更足、衣物干得更快。"
        };
        self.view.label(cx, ids!(scene_answer)).set_text(cx, scene_answer);

        let weather_outfit = daily.and_then(|d| Some((*d.temperature_2m_max.get(today_i)?, *d.temperature_2m_min.get(today_i)?)))
            .map(|(hi, lo)| if lo < 8.0 { "保暖外套 + 长裤 + 适合步行的鞋" } else if hi > 28.0 { "轻薄上衣 + 透气长裤，午后注意防晒" } else if lo < 16.0 { "薄卫衣 + 直筒长裤 + 轻便鞋" } else { "长袖上衣 + 轻便长裤" })
            .unwrap_or("天气更新后生成今日穿搭建议");
        if profile.wardrobe.trim().is_empty() {
            self.view.label(cx, ids!(outfit)).set_text(cx, weather_outfit);
            self.view.label(cx, ids!(outfit_hint)).set_text(cx, "示例建议 · 完善衣橱后可个性化");
        } else {
            self.view.label(cx, ids!(outfit)).set_text(cx, &format!("{weather_outfit} · 衣橱：{}", profile.wardrobe));
            self.view.label(cx, ids!(outfit_hint)).set_text(cx, "已根据天气结合你的衣橱");
        }
        let index = current.and_then(|v| v.time.as_ref()).and_then(|t| f.hourly.as_ref().map(|h| model::current_index(h, t))).unwrap_or(0);
        let uv = f.hourly.as_ref().and_then(|h| h.uv_index.get(index)).copied();
        let uv_tip = uv.map(|u| if u >= 6.0 { "紫外线偏强，户外活动做好防晒。" } else if u >= 3.0 { "紫外线中等，长时间户外可考虑防晒。" } else { "当前紫外线较低，留意气温和补水。" }).unwrap_or("紫外线数据暂缺。");
        let temperature = current.and_then(|v| v.apparent_temperature.or(v.temperature_2m)).unwrap_or(20.0);
        let mut health_tips: Vec<String> = Vec::new();
        for factor in &profile.factors {
            match factor.as_str() {
                "怕冷" => health_tips.push(if temperature <= 18.0 { "体感偏凉，外出建议加一层衣物。" } else { "体感温度适中，可按活动情况增减衣物。" }.into()),
                "怕热" => health_tips.push(if temperature >= 28.0 { "午后偏热，尽量避开高温时段并及时补水。" } else { "暂时不算闷热，户外活动注意补水。" }.into()),
                "关注紫外线" => health_tips.push(uv_tip.into()),
                "关注降雨" => health_tips.push(if wet { "当前有降水，外出记得带伞。" } else { "当前无降水，出门前可留意后续预报。" }.into()),
                custom => health_tips.push(format!("已记录关注：{custom}。当前天气数据暂不能自动判断这一项。")),
            }
        }
        if health_tips.is_empty() { health_tips.push(uv_tip.into()); }
        self.view.label(cx, ids!(bodycast)).set_text(cx, &health_tips.join(" "));

        if let Some(hourly) = f.hourly.as_ref() {
            for i in 0..HOURS.len() {
                let row = self.view.widget(cx, &[LiveId::from_str(HOURS[i])]);
                let n = index + i;
                let at = hourly.time.get(n).map(|t| model::hhmm(t).to_string()).unwrap_or_else(|| "—".into());
                let icon = hourly.weather_code.get(n).map(|c| model::weather_icon(*c)).unwrap_or("·");
                let t = hourly.temperature_2m.get(n).map(|v| format!("{v:.0}°")).unwrap_or_else(|| "—°".into());
                row.label(cx, ids!(at)).set_text(cx, &at);
                row.label(cx, ids!(icon)).set_text(cx, icon);
                row.label(cx, ids!(temp)).set_text(cx, &t);
            }
        }
    }

    fn set_weather_background(&mut self, cx: &mut Cx, code: Option<i32>, is_day: bool) {
        let (key, bytes) = weather_background(code, is_day);
        if self.background_key == key { return; }
        let image = self.view.widget(cx, ids!(hero)).image(cx, ids!(background));
        let loaded = image.load_image_from_data_async(cx, Path::new(key), Arc::new(bytes)).is_ok();
        image.set_visible(cx, loaded);
        self.background_key = if loaded { key.to_owned() } else { String::new() };
    }
}

impl Widget for TodayScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

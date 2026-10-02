// 日程页：一屏看今天 / 明天 / 全部，点圆圈勾掉，底部加一条。
//
// 没有本地存储，开屏用 `model::seed_schedule()` 铺一层种子；加进去的
// 条目只在内存里活到进程退出，够看框架。
use makepad_widgets::*;

use crate::model::{self, Forecast, ScheduleItem};

/// 屏上最多画的条目数。
const DAY_SLOTS: [&str; 7] = ["day0", "day1", "day2", "day3", "day4", "day5", "day6"];
const HOUR_SLOTS: [&str; 24] = [
    "hour0", "hour1", "hour2", "hour3", "hour4", "hour5", "hour6", "hour7",
    "hour8", "hour9", "hour10", "hour11", "hour12", "hour13", "hour14", "hour15",
    "hour16", "hour17", "hour18", "hour19", "hour20", "hour21", "hour22", "hour23",
];

/// 今天 / 明天 / 全部的视角。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayView {
    Today,
    Tomorrow,
    All,
    Offset(u8),
}

/// 第 `i` 个槽位的 LiveId，越界兜到最后一个。
fn slot(names: &[&str], i: usize) -> LiveId {
    LiveId::from_str(names[i.min(names.len() - 1)])
}

/// 这一页的状态：条目、当前视角。
#[derive(Clone, Debug)]
pub struct ScheduleState {
    pub items: Vec<ScheduleItem>,
    pub view: DayView,
    next_id: u32,
}

impl ScheduleState {
    pub fn new() -> Self {
        let items = model::seed_schedule();
        let next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        Self { items, view: DayView::Today, next_id }
    }

    /// 当前视角对应的日期；`All` 返回 None。
    pub fn visible_date(&self) -> Option<String> {
        match self.view {
            DayView::All => None,
            DayView::Today => Some(model::today_local()),
            DayView::Tomorrow => Some(model::next_local_date(1)),
            DayView::Offset(days) => Some(model::next_local_date(days as i64)),
        }
    }

    /// 当前视角下要显示的条目，按日期和时间排。
    pub fn visible(&self) -> Vec<ScheduleItem> {
        let date = self.visible_date();
        let mut list: Vec<ScheduleItem> = self
            .items
            .iter()
            .filter(|i| date.as_ref().map_or(true, |d| i.date == *d))
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            (a.date.as_str(), a.time.as_str()).cmp(&(b.date.as_str(), b.time.as_str()))
        });
        list
    }

    /// 视角标题。
    pub fn title(&self) -> String {
        match self.visible_date() {
            None => "全部日程".into(),
            Some(d) => format!("{} {}", model::weekday(&d), date_zh(&d)),
        }
    }

    pub fn date_for_day(&self, day: usize) -> String {
        model::next_local_date(day.min(6) as i64)
    }

    pub fn select_date(&mut self, date: &str) {
        if let Some(day) = (0..7).find(|day| self.date_for_day(*day) == date) {
            self.view = DayView::Offset(day as u8);
        }
    }

    /// 视角下的未办数量。
    pub fn pending(&self) -> usize {
        self.visible().iter().filter(|i| !i.done).count()
    }

    /// 点圆圈切换勾选。
    pub fn toggle(&mut self, id: u32) -> bool {
        let Some(item) = self.items.iter_mut().find(|i| i.id == id) else {
            return false;
        };
        item.done = !item.done;
        true
    }

    /// 加一条。`time` 空就按本地时钟补，归到今天。
    pub fn add(&mut self, title: &str, time: &str) -> bool {
        let date = model::today_local();
        let time = if time.trim().is_empty() { now_hhmm() } else { time.trim().to_string() };
        self.add_on(title, &date, &time, "")
    }

    /// Add an item at an explicit local date and time (HH:MM). The week grid
    /// stores one event per hour and renders it over that hour's weather cell.
    pub fn add_on(&mut self, title: &str, date: &str, time: &str, place: &str) -> bool {
        let title = title.trim();
        let date = date.trim();
        let time = time.trim();
        if title.is_empty() || !valid_date(date) || !valid_time(time) {
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(ScheduleItem {
            id,
            date: date.to_string(),
            time: time.to_string(),
            title: title.to_string(),
            place: place.trim().to_string(),
            done: false,
            kind: 3,
        });
        true
    }
}

fn valid_date(date: &str) -> bool {
    let mut parts = date.split('-');
    let (Some(y), Some(m), Some(d), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { return false };
    let (Ok(year), Ok(month), Ok(day)) = (y.parse::<i32>(), m.parse::<u8>(), d.parse::<u8>()) else { return false };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 || !(1..=12).contains(&month) { return false; }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=month_days).contains(&day)
}

fn valid_time(time: &str) -> bool {
    let mut parts = time.split(':');
    let (Some(h), Some(m), None) = (parts.next(), parts.next(), parts.next()) else { return false };
    h.len() == 2 && h.parse::<u8>().is_ok_and(|h| h < 24)
        && m.len() == 2 && m.parse::<u8>().is_ok_and(|m| m < 60)
}

/// "2026-09-27" → "9月27日 周一"
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

/// 本地时钟的 "HH:MM"，给没填时间的条目兜底。
fn now_hhmm() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let total = (secs + 8 * 3600) % 86400;
    format!("{:02}:{:02}", total / 3600, (total % 3600) / 60)
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let CardTitle = Label{
        padding: 0 margin: 0
        draw_text +: { color: #1B1F27 text_style: theme.font_bold{font_size: 12.5} }
    }
    let Sub = Label{
        padding: 0 margin: 0
        draw_text +: { color: #5B6472 text_style: theme.font_regular{font_size: 10.5} }
    }
    let Tiny = Label{
        padding: 0 margin: 0
        draw_text +: { color: #8B95A6 text_style: theme.font_regular{font_size: 9.0} }
    }

    let CardFlat = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 0
        padding: Inset{top: 11 left: 12 right: 12 bottom: 11}
        show_bg: true
        draw_bg +: {
            color: #FFFFFF
            border_radius: 16.0
            border_size: 0.6
            border_color: #E6EAF0
            shadow_color: #0B1F3A10
            shadow_radius: 8.0
        }
    }

    // 条目一行：勾圈 + 时间 + 标题/地点。
    let ItemRow = View{
        width: Fill height: Fit flow: Right spacing: 9
        padding: Inset{top: 8 left: 10 right: 10 bottom: 8}
        margin: 0
        align: Align{y: 0.5}
        show_bg: true
        draw_bg +: { color: #FFFFFF border_radius: 12.0 border_size: 0.6 border_color: #E6EAF0 }

        check := View{
            width: 18 height: 18
            padding: 0 margin: 0
            align: Align{x: 0.5 y: 0.5}
            cursor: MouseCursor.Hand
            show_bg: true
            draw_bg +: { color: #FFFFFF border_radius: 9.0 border_size: 1.2 border_color: #C7CEDA }
            mark := Label{
                padding: 0 margin: 0 width: Fit height: Fit
                align: Align{x: 0.5 y: 0.5}
                visible: false
                text: "✓"
                draw_text +: { color: #3D7BFA text_style: theme.font_bold{font_size: 10} }
            }
        }
        time := Label{
            width: 34 height: Fit padding: 0 margin: 0
            align: Align{x: 0.0 y: 0.5}
            draw_text +: { color: #1E58D9 text_style: theme.font_bold{font_size: 10.5} }
        }
        body := View{ width: Fill height: Fit flow: Down spacing: 2
            title := Label{
                width: Fill height: Fit padding: 0 margin: 0
                max_lines: 1
                draw_text +: { color: #1B1F27 text_style: theme.font_bold{font_size: 11.0} }
            }
            place := Label{
                width: Fill height: Fit padding: 0 margin: 0
                max_lines: 1
                draw_text +: { color: #8B95A6 text_style: theme.font_regular{font_size: 9.5} }
            }
        }
    }

    // 日期视角胶囊。
    let DayChip = View{
        width: Fit height: 28
        padding: Inset{top: 0 left: 11 right: 11 bottom: 0}
        margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #ECF1F7 border_radius: 14.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            draw_text +: { color: #5B6472 text_style: theme.font_bold{font_size: 10.0} }
        }
    }

    // 加一条的按钮。
    let ActionBtn = View{
        width: Fit height: 34
        padding: Inset{top: 0 left: 13 right: 13 bottom: 0}
        margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #3D7BFA border_radius: 17.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            draw_text +: { color: #FFFFFF text_style: theme.font_bold{font_size: 11.0} }
        }
    }

    let HourRow = View{
        width: Fill height: 29 flow: Right spacing: 5
        align: Align{y: 0.5}
        hour := Label{
            width: 36 height: Fit padding: 0 margin: 0
            draw_text +: { color: #657286 text_style: theme.font_bold{font_size: 9.0} }
        }
        weather := View{
            width: Fill height: 25 flow: Right spacing: 5
            padding: Inset{top: 3 left: 6 right: 6 bottom: 3}
            align: Align{y: 0.5}
            show_bg: true
            draw_bg +: { color: #F0F3F6 border_radius: 7.0 }
            condition := Label{
                width: 44 height: Fit padding: 0 margin: 0
                draw_text +: { color: #354152 text_style: theme.font_bold{font_size: 8.5} }
            }
            detail := Label{
                width: Fill height: Fit padding: 0 margin: 0
                draw_text +: { color: #566477 text_style: theme.font_regular{font_size: 8.5} }
            }
            plan := Label{
                width: Fill height: Fit padding: 0 margin: 0
                max_lines: 1
                draw_text +: { color: #26354A text_style: theme.font_bold{font_size: 8.0} }
            }
        }
    }

    mod.widgets.ScheduleScreenBase = #(ScheduleScreen::register_widget(vm))
    mod.widgets.ScheduleScreen = set_type_default() do mod.widgets.ScheduleScreenBase{
        width: Fill height: Fill

        ScrollYView{
            width: Fill height: Fill flow: Down spacing: 9
            padding: Inset{top: 0 left: 12 right: 12 bottom: 12}
            show_bg: false
            scroll_bars +: { show_scroll_x: false }

            CardFlat{
                flow: Down
                spacing: 8
                CardTitle{ text: "我的天气日程" }
                Sub{ text: "把要做的事放进天气里，按小时挑选合适的出门时间" }
                View{ width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5}
                    day0 := DayChip{ name.text: "一" }
                    day1 := DayChip{ name.text: "二" }
                    day2 := DayChip{ name.text: "三" }
                    day3 := DayChip{ name.text: "四" }
                    day4 := DayChip{ name.text: "五" }
                    day5 := DayChip{ name.text: "六" }
                    day6 := DayChip{ name.text: "日" }
                }
            }

            sub := Sub{ width: Fill margin: Inset{top: 2} }
            count := Tiny{ width: Fill text: "" }

            hour0 := HourRow{}
            hour1 := HourRow{}
            hour2 := HourRow{}
            hour3 := HourRow{}
            hour4 := HourRow{}
            hour5 := HourRow{}
            hour6 := HourRow{}
            hour7 := HourRow{}
            hour8 := HourRow{}
            hour9 := HourRow{}
            hour10 := HourRow{}
            hour11 := HourRow{}
            hour12 := HourRow{}
            hour13 := HourRow{}
            hour14 := HourRow{}
            hour15 := HourRow{}
            hour16 := HourRow{}
            hour17 := HourRow{}
            hour18 := HourRow{}
            hour19 := HourRow{}
            hour20 := HourRow{}
            hour21 := HourRow{}
            hour22 := HourRow{}
            hour23 := HourRow{}

            Tiny{ width: Fill margin: Inset{top: 6} text: "日程写入对应时段；天气预报最多覆盖未来 10 天。" }
        }

        CardFlat{
            flow: Right
            align: Align{y: 0.5}
            padding: Inset{top: 7 left: 7 right: 7 bottom: 7}
            date_input := TextInput{
                width: 106 height: 34
                empty_text: "YYYY-MM-DD"
                draw_text +: { color: #1B1F27 color_focus: #1B1F27 color_empty: #9AA3B2 color_empty_focus: #9AA3B2 text_style: theme.font_regular{font_size: 9.0} }
            }
            time_input := TextInput{
                width: 58 height: 34
                empty_text: "HH:MM"
                draw_text +: { color: #1B1F27 color_focus: #1B1F27 color_empty: #9AA3B2 color_empty_focus: #9AA3B2 text_style: theme.font_regular{font_size: 9.0} }
            }
            input := TextInput{
                width: Fill height: 34
                empty_text: "新建日程"
                draw_text +: {
                    color: #1B1F27
                    color_focus: #1B1F27
                    color_empty: #9AA3B2
                    color_empty_focus: #9AA3B2
                    text_style: theme.font_regular{font_size: 11.5}
                }
            }
            add := ActionBtn{ name.text: "添加" }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ScheduleScreen {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
}

impl ScheduleScreen {
    fn hour(&self, cx: &mut Cx, i: usize) -> WidgetRef {
        self.view.widget(cx, &[slot(&HOUR_SLOTS, i)])
    }

    /// 输入框。
    pub fn input(&self, cx: &mut Cx) -> TextInputRef {
        self.view.text_input(cx, ids!(input))
    }

    pub fn date_input(&self, cx: &mut Cx) -> TextInputRef { self.view.text_input(cx, ids!(date_input)) }
    pub fn time_input(&self, cx: &mut Cx) -> TextInputRef { self.view.text_input(cx, ids!(time_input)) }

    /// 点没点「添加」。
    pub fn add_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        self.view.view(cx, ids!(add)).finger_up(actions).is_some()
    }

    /// 点中了第几个日期视角。
    pub fn day_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..DAY_SLOTS.len() {
            if self.view.view(cx, &[slot(&DAY_SLOTS, i)]).finger_up(actions).is_some() {
                return Some(i);
            }
        }
        None
    }

    /// Paint the 24 hourly weather cells and overlay scheduled activities.
    pub fn render(&mut self, cx: &mut Cx, state: &ScheduleState, forecast: &Option<Forecast>) {
        let date = state.visible_date().unwrap_or_else(model::today_local);
        let sub = self.view.label(cx, ids!(sub));
        let sub_text = format!("{} · {} · {} 条日程", model::weekday(&date), date, state.visible().len());
        crate::ui::set_label(&sub, cx, &sub_text, crate::ui::INK_2);

        let count = self.view.label(cx, ids!(count));
        crate::ui::set_label(
            &count,
            cx,
            &format!("天气颜色图例：晴黄 · 多云灰绿 · 雨蓝 · 雪青 · 雷紫"),
            crate::ui::INK_3,
        );

        // The seven day chips are a compact week header.
        for i in 0..DAY_SLOTS.len() {
            let day = state.date_for_day(i);
            let sel = date == day;
            let chip = self.view.widget(cx, &[slot(&DAY_SLOTS, i)]);
            chip.label(cx, &[live_id!(name)]).set_text(cx, &format!("{}{}", model::weekday(&day).trim_start_matches("周"), &day[8..]));
            crate::ui::fill(
                &chip,
                cx,
                if sel { vec4(0.918, 0.945, 0.996, 1.0) } else { vec4(0.925, 0.945, 0.969, 1.0) },
            );
            crate::ui::tint(
                &chip.label(cx, &[live_id!(name)]),
                cx,
                if sel { vec4(0.118, 0.345, 0.851, 1.0) } else { vec4(0.357, 0.392, 0.447, 1.0) },
            );
        }

        let hourly = forecast.as_ref().and_then(|f| f.hourly.as_ref());
        for hour in 0..24 {
            let row = self.hour(cx, hour);
            let label = row.label(cx, &[live_id!(hour)]);
            label.set_text(cx, &format!("{:02}:00", hour));
            let weather = row.child_by_path(&[live_id!(weather)]);
            let weather_index = hourly.and_then(|data| data.time.iter().position(|time| {
                model::date_of(time) == date && model::hhmm(time).starts_with(&format!("{:02}:", hour))
            }));
            let (code, temp, rain) = weather_index.map(|i| (
                hourly.and_then(|h| h.weather_code.get(i).copied()),
                hourly.and_then(|h| h.temperature_2m.get(i).copied()),
                hourly.and_then(|h| h.precipitation.get(i).copied()),
            )).unwrap_or((None, None, None));
            let color = weather_color(code, rain);
            crate::ui::fill(&weather, cx, color);
            let condition = weather.label(cx, &[live_id!(condition)]);
            condition.set_text(cx, code.map(model::condition).unwrap_or("暂无预报"));
            let detail = weather.label(cx, &[live_id!(detail)]);
            let detail_text = match (temp, rain) {
                (Some(t), Some(r)) if r > 0.0 => format!("{t:.0}° · {r:.1}mm"),
                (Some(t), _) => format!("{t:.0}°"),
                _ => String::new(),
            };
            detail.set_text(cx, &detail_text);
            let plans = state.items.iter().filter(|item| item.date == date && item.time.get(..2).and_then(|s| s.parse::<usize>().ok()) == Some(hour))
                .map(|item| item.title.as_str()).collect::<Vec<_>>().join("、");
            weather.label(cx, &[live_id!(plan)]).set_text(cx, &plans);
        }
    }
}

fn weather_color(code: Option<i32>, rain: Option<f64>) -> Vec4 {
    match code.unwrap_or(-1) {
        0 | 1 => vec4(1.0, 0.91, 0.52, 1.0),
        2 => vec4(0.79, 0.88, 0.83, 1.0),
        3 | 45 | 48 => vec4(0.82, 0.86, 0.89, 1.0),
        51..=67 | 80..=82 => match rain.unwrap_or(0.0) {
            x if x >= 5.0 => vec4(0.24, 0.48, 0.78, 1.0),
            x if x >= 2.0 => vec4(0.42, 0.67, 0.88, 1.0),
            x if x > 0.0 => vec4(0.68, 0.83, 0.94, 1.0),
            _ => vec4(0.83, 0.91, 0.97, 1.0),
        },
        71..=77 | 85..=86 => vec4(0.75, 0.91, 0.95, 1.0),
        95..=99 => vec4(0.75, 0.69, 0.91, 1.0),
        _ => vec4(0.93, 0.94, 0.96, 1.0),
    }
}

impl Widget for ScheduleScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope)
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_starts_on_today_with_seeds() {
        let state = ScheduleState::new();
        assert_eq!(state.view, DayView::Today);
        assert!(!state.visible().is_empty());
    }

    #[test]
    fn the_day_views_filter_by_date() {
        let mut state = ScheduleState::new();
        state.view = DayView::All;
        let all = state.visible();
        assert!(all.len() >= 2);

        state.view = DayView::Today;
        let today = state.visible();
        for item in &today {
            assert_eq!(item.date, model::today_local());
        }

        state.view = DayView::Tomorrow;
        for item in &state.visible() {
            assert_eq!(item.date, model::next_local_date(1));
        }
    }

    #[test]
    fn toggling_flips_the_done_flag() {
        let mut state = ScheduleState::new();
        let Some(id) = state.items.first().map(|i| i.id) else { return };
        let before = state.items.iter().find(|i| i.id == id).unwrap().done;
        state.toggle(id);
        assert_eq!(state.items.iter().find(|i| i.id == id).unwrap().done, !before);
    }

    #[test]
    fn toggling_an_unknown_id_is_a_noop() {
        let mut state = ScheduleState::new();
        assert!(!state.toggle(99_999));
    }

    #[test]
    fn adding_gives_a_missing_time_and_today() {
        let mut state = ScheduleState::new();
        let before = state.items.len();
        // 空白标题被拒：返回 false，列表不变。
        assert!(!state.add("   ", "09:00"));
        assert_eq!(state.items.len(), before);
        assert!(state.add("牙医复诊", ""));
        assert_eq!(state.items.len(), before + 1);
        let added = state.items.last().unwrap();
        assert_eq!(added.title, "牙医复诊");
        assert_eq!(added.date, model::today_local());
        assert!(!added.time.is_empty());
    }

    #[test]
    fn adding_keeps_the_given_time() {
        let mut state = ScheduleState::new();
        state.add("周会", " 09:30 ");
        assert_eq!(state.items.last().unwrap().time, "09:30");
    }

    #[test]
    fn the_visible_list_is_sorted_by_time() {
        let mut state = ScheduleState::new();
        state.view = DayView::All;
        let list = state.visible();
        for pair in list.windows(2) {
            assert!(
                (pair[0].date.as_str(), pair[0].time.as_str()) <= (pair[1].date.as_str(), pair[1].time.as_str())
            );
        }
    }

    #[test]
    fn the_date_zh_reads_chinese() {
        assert!(date_zh("2026-09-27").starts_with("9月27日"));
        assert_eq!(date_zh("junk"), "junk");
    }

    #[test]
    fn the_now_hhmm_has_two_digits_each() {
        let s = now_hhmm();
        assert_eq!(s.len(), 5);
        assert_eq!(&s[2..3], ":");
    }

    #[test]
    fn the_slot_lookup_wraps_like_the_others() {
        assert_eq!(slot(&HOUR_SLOTS, 4), LiveId::from_str("hour4"));
        assert_eq!(slot(&HOUR_SLOTS, 99), LiveId::from_str("hour23"));
    }
}

// City switching flow: saved cities -> popular cities/search -> geocoding results.
use makepad_widgets::*;
use crate::{model, ui};

const CITY_SLOTS: [&str; 16] = ["c0","c1","c2","c3","c4","c5","c6","c7","c8","c9","c10","c11","c12","c13","c14","c15"];
const RESULT_SLOTS: [&str; 20] = ["r0","r1","r2","r3","r4","r5","r6","r7","r8","r9","r10","r11","r12","r13","r14","r15","r16","r17","r18","r19"];
const POPULAR_SLOTS: [&str; 18] = ["d0","d1","d2","d3","d4","d5","d6","d7","d8","i0","i1","i2","i3","i4","i5","i6","i7","i8"];

fn id(names: &[&str], i: usize) -> LiveId { LiveId::from_str(names[i.min(names.len()-1)]) }

fn international_cities() -> Vec<model::City> {
    [
        ("纽约", "美国", 40.7128, -74.0060), ("巴黎", "法国", 48.8566, 2.3522),
        ("伦敦", "英国", 51.5072, -0.1276), ("东京", "日本", 35.6762, 139.6503),
        ("洛杉矶", "美国", 34.0522, -118.2437), ("首尔", "韩国", 37.5665, 126.9780),
        ("悉尼", "澳大利亚", -33.8688, 151.2093), ("多伦多", "加拿大", 43.6532, -79.3832),
        ("曼谷", "泰国", 13.7563, 100.5018),
    ].into_iter().map(|(name, region, lat, lon)| model::City::custom(name.into(), region.into(), lat, lon)).collect()
}

pub fn international_cities_for_shell(index: usize) -> Option<model::City> {
    international_cities().get(index).cloned()
}

#[derive(Clone, Copy, PartialEq)]
enum PickerMode { Cities, Discover }

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let Muted = Label{ width: Fill height: Fit padding: 0 margin: 0 max_lines: 2
        draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 10.0} } }
    let Heading = Label{ width: Fit height: Fit padding: 0 margin: 0
        draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 20.0} } }
    let BackBtn = RoundedShadowView{ width: 36 height: 36 align: Align{x: 0.5 y: 0.5} cursor: MouseCursor.Hand show_bg: true
        draw_bg +: { color: #303030 border_radius: 18.0 }
        text := Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 0 margin: 0 text: "‹"
            draw_text +: { color: #F6F2EA text_style: theme.font_regular{font_size: 24.0} } } }
    let SearchEntry = RoundedShadowView{ width: Fill height: 46 flow: Right spacing: 10 padding: Inset{left: 15 right: 15}
        align: Align{y: 0.5} cursor: MouseCursor.Hand show_bg: true
        draw_bg +: { color: #303030 border_radius: 23.0 }
        glass := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "⌕"
            draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 19.0} } }
        text := Label{ width: Fill height: Fit padding: 0 margin: 0 text: "搜索城市或地区"
            draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 11.5} } } }
    let SavedRow = RoundedShadowView{ width: Fill height: 70 flow: Right spacing: 10 padding: Inset{left: 16 right: 16}
        align: Align{y: 0.5} cursor: MouseCursor.Hand show_bg: true
        draw_bg +: { color: #252525 border_radius: 15.0 border_size: 1.0 border_color: #383838 }
        stack := View{ width: Fill height: Fit flow: Down spacing: 4 align: Align{y: 0.5}
            name := Label{width: Fill height: Fit padding: 0 margin: 0
                draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 14.0} } }
            detail := Muted{} }
        marker := Label{width: Fit height: Fit padding: 0 margin: 0
            draw_text +: { color: #F3A44D text_style: theme.font_regular{font_size: 10.0} } } }
    let PopularChip = RoundedShadowView{ width: Fill height: 42 align: Align{x: 0.5 y: 0.5} cursor: MouseCursor.Hand show_bg: true
        draw_bg +: { color: #303030 border_radius: 21.0 }
        name := Label{width: Fit height: Fit padding: 0 margin: 0 align: Align{x: 0.5 y: 0.5}
            draw_text +: { color: #EDE9E2 text_style: theme.font_regular{font_size: 11.5} } } }
    let ResultRow = RoundedShadowView{ width: Fill height: 52 flow: Down spacing: 3 padding: Inset{left: 4 right: 4}
        align: Align{y: 0.5} cursor: MouseCursor.Hand show_bg: true
        draw_bg +: { color: #191919 border_radius: 0.0 border_size: 0.0 }
        line := View{width: Fill height: Fit flow: Right spacing: 0
            before := Label{width: Fit height: Fit padding: 0 margin: 0
                draw_text +: { color: #F6F2EA text_style: theme.font_regular{font_size: 13.0} } }
            matched := Label{width: Fit height: Fit padding: 0 margin: 0
                draw_text +: { color: #52A7FF text_style: theme.font_bold{font_size: 13.0} } }
            after := Label{width: Fit height: Fit padding: 0 margin: 0
                draw_text +: { color: #F6F2EA text_style: theme.font_regular{font_size: 13.0} } } }
        detail := Muted{} }

    mod.widgets.PlacePickerScreenBase = #(PlacePickerScreen::register_widget(vm))
    mod.widgets.PlacePickerScreen = set_type_default() do mod.widgets.PlacePickerScreenBase{
        width: Fill height: Fill flow: Down spacing: 14 padding: Inset{top: 18 left: 16 right: 16 bottom: 14}
        show_bg: true draw_bg +: { color: #191919 }
        View{width: Fill height: Fit flow: Right spacing: 12 align: Align{y: 0.5}
            back := BackBtn{}
            View{width: Fill height: Fit flow: Down spacing: 3
                eyebrow := Muted{width: Fit text: "城市管理"}
                title := Heading{text: "我的城市"} }
            close := RoundedShadowView{width: 36 height: 36 align: Align{x: 0.5 y: 0.5} cursor: MouseCursor.Hand show_bg: true
                draw_bg +: {color: #303030 border_radius: 18.0}
                x := Label{width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 0 margin: 0 text: "×"
                    draw_text +: {color: #AAA69E text_style: theme.font_regular{font_size: 17.0}}} }
        }
        management := View{width: Fill height: Fill flow: Down spacing: 12
            search_entry := SearchEntry{}
            section := Muted{width: Fit text: "已保存的城市 · 点击切换"}
            ScrollYView{width: Fill height: Fill flow: Down spacing: 9 show_bg: true draw_bg +: {color: #191919}
                scroll_bars +: {show_scroll_x: false}
                c0 := SavedRow{visible: false} c1 := SavedRow{visible: false}
                c2 := SavedRow{visible: false} c3 := SavedRow{visible: false}
                c4 := SavedRow{visible: false} c5 := SavedRow{visible: false}
                c6 := SavedRow{visible: false} c7 := SavedRow{visible: false}
                c8 := SavedRow{visible: false} c9 := SavedRow{visible: false}
                c10 := SavedRow{visible: false} c11 := SavedRow{visible: false}
                c12 := SavedRow{visible: false} c13 := SavedRow{visible: false}
                c14 := SavedRow{visible: false} c15 := SavedRow{visible: false}
            }
        }
        discovery := View{width: Fill height: Fill flow: Down spacing: 13 visible: false
            View{width: Fill height: Fit flow: Right spacing: 9 align: Align{y: 0.5}
                search_input := TextInput{width: Fill height: 44 empty_text: "城市名称，如：晋"
                    draw_bg +: {color: #303030 color_empty: #303030 color_hover: #303030 color_focus: #303030 color_down: #303030 border_radius: 22.0}
                    draw_text +: {color: #F2EEE6 color_focus: #F2EEE6 color_empty: #AAA69E color_empty_focus: #AAA69E text_style: theme.font_regular{font_size: 11.5}} }
                cancel := View{width: Fit height: Fit cursor: MouseCursor.Hand
                    text := Label{width: Fit height: Fit padding: 0 margin: 0 text: "取消"
                        draw_text +: {color: #52A7FF text_style: theme.font_regular{font_size: 11.5}}}}
            }
            popular := View{width: Fill height: Fill flow: Down spacing: 12
                domestic_title := Muted{width: Fit text: "国内热门城市"}
                View{width: Fill height: Fit flow: Right spacing: 8
                    d0 := PopularChip{} d1 := PopularChip{} d2 := PopularChip{} }
                View{width: Fill height: Fit flow: Right spacing: 8
                    d3 := PopularChip{} d4 := PopularChip{} d5 := PopularChip{} }
                View{width: Fill height: Fit flow: Right spacing: 8
                    d6 := PopularChip{} d7 := PopularChip{} d8 := PopularChip{} }
                international_title := Muted{width: Fit text: "国际热门城市"}
                View{width: Fill height: Fit flow: Right spacing: 8
                    i0 := PopularChip{} i1 := PopularChip{} i2 := PopularChip{} }
                View{width: Fill height: Fit flow: Right spacing: 8
                    i3 := PopularChip{} i4 := PopularChip{} i5 := PopularChip{} }
                View{width: Fill height: Fit flow: Right spacing: 8
                    i6 := PopularChip{} i7 := PopularChip{} i8 := PopularChip{} }
            }
            results := View{width: Fill height: Fill flow: Down spacing: 2 visible: false
                result_status := Muted{width: Fill}
                ScrollYView{width: Fill height: Fill flow: Down spacing: 0 show_bg: true draw_bg +: {color: #191919}
                    scroll_bars +: {show_scroll_x: false}
                    r0 := ResultRow{visible: false} r1 := ResultRow{visible: false}
                    r2 := ResultRow{visible: false} r3 := ResultRow{visible: false}
                    r4 := ResultRow{visible: false} r5 := ResultRow{visible: false}
                    r6 := ResultRow{visible: false} r7 := ResultRow{visible: false}
                    r8 := ResultRow{visible: false} r9 := ResultRow{visible: false}
                    r10 := ResultRow{visible: false} r11 := ResultRow{visible: false}
                    r12 := ResultRow{visible: false} r13 := ResultRow{visible: false}
                    r14 := ResultRow{visible: false} r15 := ResultRow{visible: false}
                    r16 := ResultRow{visible: false} r17 := ResultRow{visible: false}
                    r18 := ResultRow{visible: false} r19 := ResultRow{visible: false}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct PlacePickerScreen {
    #[source] source: ScriptObjectRef,
    #[deref] view: View,
    #[rust(PickerMode::Cities)] mode: PickerMode,
}

impl PlacePickerScreen {
    fn saved_row(&self, cx: &mut Cx, i: usize) -> WidgetRef { self.view.widget(cx, &[id(&CITY_SLOTS, i)]) }
    fn result_row(&self, cx: &mut Cx, i: usize) -> WidgetRef { self.view.widget(cx, &[id(&RESULT_SLOTS, i)]) }
    fn popular_chip(&self, cx: &mut Cx, i: usize) -> WidgetRef { self.view.widget(cx, &[id(&POPULAR_SLOTS, i)]) }
    pub fn open_search_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.view(cx, ids!(search_entry)).finger_up(actions).is_some() }
    pub fn back_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.view(cx, ids!(back)).finger_up(actions).is_some() }
    pub fn cancel_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.view(cx, ids!(cancel)).finger_up(actions).is_some() }
    pub fn close_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.view(cx, ids!(close)).finger_up(actions).is_some() }
    pub fn search_input(&self, cx: &mut Cx) -> TextInputRef { self.view.text_input(cx, ids!(search_input)) }
    pub fn saved_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..CITY_SLOTS.len() { if self.view.view(cx, &[id(&CITY_SLOTS, i)]).finger_up(actions).is_some() { return Some(i); } } None
    }
    pub fn result_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..RESULT_SLOTS.len() { if self.view.view(cx, &[id(&RESULT_SLOTS, i)]).finger_up(actions).is_some() { return Some(i); } } None
    }
    pub fn popular_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..POPULAR_SLOTS.len() { if self.view.view(cx, &[id(&POPULAR_SLOTS, i)]).finger_up(actions).is_some() { return Some(i); } } None
    }
    pub fn is_discovering(&self) -> bool { self.mode == PickerMode::Discover }
    pub fn enter_discover(&mut self, cx: &mut Cx) {
        self.mode = PickerMode::Discover;
        self.search_input(cx).set_text(cx, "");
        self.view.view(cx, ids!(management)).set_visible(cx, false);
        self.view.view(cx, ids!(discovery)).set_visible(cx, true);
        self.view.view(cx, ids!(popular)).set_visible(cx, true);
        self.view.view(cx, ids!(results)).set_visible(cx, false);
        self.view.label(cx, ids!(title)).set_text(cx, "添加城市");
        self.view.label(cx, ids!(eyebrow)).set_text(cx, "搜索或从热门城市中选择");
        self.search_input(cx).set_key_focus(cx);
    }
    pub fn set_searching(&mut self, cx: &mut Cx, searching: bool) {
        self.view.view(cx, ids!(popular)).set_visible(cx, !searching);
        self.view.view(cx, ids!(results)).set_visible(cx, searching);
    }
    pub fn show_cities(&mut self, cx: &mut Cx) {
        self.mode = PickerMode::Cities;
        self.view.view(cx, ids!(management)).set_visible(cx, true);
        self.view.view(cx, ids!(discovery)).set_visible(cx, false);
        self.view.label(cx, ids!(title)).set_text(cx, "我的城市");
        self.view.label(cx, ids!(eyebrow)).set_text(cx, "城市管理");
    }
    pub fn render(&mut self, cx: &mut Cx, st: &crate::weather::WeatherState) {
        let saved = &st.saved_cities;
        for i in 0..CITY_SLOTS.len() {
            let row = self.saved_row(cx, i);
            let Some(city) = saved.get(i) else { row.set_visible(cx, false); continue; };
            row.set_visible(cx, true);
            row.label(cx, &[live_id!(name)]).set_text(cx, city.name.as_ref());
            row.label(cx, &[live_id!(detail)]).set_text(cx, city.en.as_ref());
            row.label(cx, &[live_id!(marker)]).set_text(cx, if st.city().lat == city.lat && st.city().lon == city.lon { "当前城市" } else { "" });
        }
        for i in 0..9 {
            let chip = self.popular_chip(cx, i);
            chip.label(cx, ids!(name)).set_text(cx, model::CITIES[i].name.as_ref());
        }
        let international = international_cities();
        for (i, city) in international.iter().enumerate() {
            self.popular_chip(cx, i + 9).label(cx, ids!(name)).set_text(cx, city.name.as_ref());
        }
        let status = self.view.label(cx, ids!(result_status));
        status.set_text(cx, &st.place_status);
        for i in 0..RESULT_SLOTS.len() {
            let row = self.result_row(cx, i);
            let Some(city) = st.place_results.get(i) else { row.set_visible(cx, false); continue; };
            row.set_visible(cx, true);
            let full = if city.en.is_empty() { city.name.to_string() } else { format!("{}-{}", city.name, city.en.replace(" · ", "-")) };
            let query = self.search_input(cx).text();
            let found = full.find(&query).filter(|_| !query.is_empty());
            let (before, matched, after) = if let Some(start) = found {
                let end = start + query.len();
                (&full[..start], &full[start..end], &full[end..])
            } else { (full.as_str(), "", "") };
            let line = row.widget(cx, ids!(line));
            line.label(cx, ids!(before)).set_text(cx, before);
            line.label(cx, ids!(matched)).set_text(cx, matched);
            line.label(cx, ids!(after)).set_text(cx, after);
            row.label(cx, ids!(detail)).set_text(cx, &format!("{:.3}°, {:.3}°", city.lat, city.lon));
        }
        ui::tint(&status, cx, vec4(0.667, 0.651, 0.620, 1.0));
        self.view.redraw(cx);
    }
}

impl Widget for PlacePickerScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) { self.view.handle_event(cx, event, scope) }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep { self.view.draw_walk(cx, scope, walk) }
}

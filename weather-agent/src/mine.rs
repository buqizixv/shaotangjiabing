//! Personal weather profile and a compact agenda overview.
use makepad_widgets::*;

use crate::schedule::ScheduleState;

const ITEMS: [&str; 3] = ["i0", "i1", "i2"];
pub const PREF_FACTORS: [&str; 4] = ["怕冷", "怕热", "关注紫外线", "关注降雨"];

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let Muted = Label{
        width: Fill height: Fit padding: 0 margin: 0 max_lines: 2
        draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 10.0} }
    }
    let Title = Label{
        width: Fill height: Fit padding: 0 margin: 0
        draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 13.0} }
    }
    let Card = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 8
        padding: Inset{top: 14 left: 15 right: 15 bottom: 14}
        show_bg: true
        draw_bg +: { color: #242424 border_radius: 18.0 border_size: 0.7 border_color: #383838 }
    }
    let ProfileRow = View{
        width: Fill height: Fit flow: Overlay cursor: MouseCursor.Hand
        body := View{width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
            padding: Inset{top: 9 left: 10 right: 10 bottom: 9}
            show_bg: true
            draw_bg +: { color: #2C2C2C border_radius: 13.0 }
            icon := View{ width: 30 height: 30 align: Align{x: 0.5 y: 0.5} show_bg: true
                draw_bg +: { color: #493521 border_radius: 15.0 }
                glyph := Label{ width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 0 margin: 0
                    draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 12.0} } }
            }
            content := View{ width: Fill height: Fit flow: Down spacing: 3
                title := Title{}
                detail := Muted{}
            }
            status := Muted{ width: Fit text: "待设置" }
        }
        click := ButtonFlat{width: Fill height: Fill text: "" padding: 0 margin: 0
            draw_bg +: {color: uniform(#00000000) color_hover: uniform(#00000000) color_down: uniform(#00000000) color_focus: uniform(#00000000)
                border_size: uniform(0.0) border_color: uniform(#00000000) border_color_hover: uniform(#00000000) border_color_down: uniform(#00000000) border_color_focus: uniform(#00000000)}
            draw_text +: {color: #00000000 text_style: theme.font_regular{font_size: 1.0}}
        }
    }
    let AgendaRow = View{
        width: Fill height: Fit flow: Right spacing: 9 align: Align{y: 0.5}
        padding: Inset{top: 8 left: 9 right: 9 bottom: 8}
        show_bg: true
        draw_bg +: { color: #2C2C2C border_radius: 12.0 }
        at := Label{ width: 42 height: Fit padding: 0 margin: 0
            draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 10.0} } }
        View{ width: Fill height: Fit flow: Down spacing: 2
            title := Title{}
            place := Muted{}
        }
    }
    let ActionButton = Button{width: Fit height: 38 padding: Inset{left: 14 right: 14} draw_bg +: {color: #F3A44D color_hover: #FFB45B color_down: #DA8739 border_radius: 10.0} draw_text +: {color: #21180F text_style: theme.font_bold{font_size: 12.0}}}
    let Field = TextInputFlat{width: Fill height: 42 empty_text: "输入内容" padding: Inset{left: 12 right: 12} draw_bg +: {color: #303030 color_focus: #303030 border_radius: 10.0 border_size: 1.0 border_color: #484848 border_color_focus: #F3A44D} draw_text +: {color: #F6F2EA color_focus: #F6F2EA color_empty: #AAA69E text_style: theme.font_regular{font_size: 12.0}}}

    mod.widgets.MineScreenBase = #(MineScreen::register_widget(vm))
    mod.widgets.MineScreen = set_type_default() do mod.widgets.MineScreenBase{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: { color: #191919 }
        ScrollYView{
            width: Fill height: Fill flow: Down spacing: 10
            padding: Inset{top: 15 left: 14 right: 14 bottom: 14}
            show_bg: true
            draw_bg +: { color: #191919 }
            scroll_bars +: { show_scroll_x: false }

            View{ width: Fill height: Fit flow: Down spacing: 4
                Label{ padding: 0 margin: 0 width: Fill height: Fit text: "我的"
                    draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 21.0} } }
                Muted{ text: "你的天气偏好与日常安排" }
            }

            Card{
                Title{ text: "让知时更懂你" }
                Muted{ text: "资料由你主动填写。尚未设置时，不生成个人化推断。" }
                wardrobe := ProfileRow{ body.icon.glyph.text: "衣" body.content.title.text: "我的衣橱" body.content.detail.text: "添加常穿单品，获得具体搭配建议" }
                health := ProfileRow{ body.icon.glyph.text: "身" body.content.title.text: "身体气象站" body.content.detail.text: "设置你想关注的天气因素" }
                care := ProfileRow{ body.icon.glyph.text: "牵" body.content.title.text: "跨城牵挂" body.content.detail.text: "添加家人所在城市，查看当地天气" }
            }

            editor := View{ visible: false width: Fill height: Fit flow: Down spacing: 8
                padding: Inset{top: 14 left: 15 right: 15 bottom: 14}
                show_bg: true
                draw_bg +: { color: #242424 border_radius: 18.0 border_size: 0.7 border_color: #383838 }
                editor_title := Title{ text: "设置" }
                editor_hint := Muted{ text: "资料只用于个性化天气建议" }
                wardrobe_input := View{visible: false width: Fill height: Fit input := Field{empty_text: "例如：薄外套、运动鞋、雨伞"}}
                wardrobe_save := View{ width: Fill height: Fit save_btn := ActionButton{text: "保存衣橱"} }
                factors := View{ visible: false width: Fill height: Fit flow: Down spacing: 7
                    factor_row := View{width: Fill height: Fit flow: Right spacing: 6
                        factor0 := ActionButton{text: "怕冷"}
                        factor1 := ActionButton{text: "怕热"}
                        factor2 := ActionButton{text: "紫外线"}
                        factor3 := ActionButton{text: "降雨"}
                        custom_factor_plus := ActionButton{text: "+"}
                    }
                    Muted{text: "点 + 添加自定义关注标签；点自定义标签移除"}
                    custom_factors := View{width: Fill height: Fit flow: Right spacing: 6
                        custom0 := ActionButton{visible: false text: ""}
                        custom1 := ActionButton{visible: false text: ""}
                        custom2 := ActionButton{visible: false text: ""}
                        custom3 := ActionButton{visible: false text: ""}
                        custom4 := ActionButton{visible: false text: ""}
                    }
                    custom_factor_entry := View{visible: false width: Fill height: Fit flow: Right spacing: 6
                        custom_factor_input := Field{width: Fill empty_text: "输入自定义关注标签"}
                        custom_factor_add := ActionButton{text: "添加标签"}
                    }
                    custom_factor_status := Muted{}
                }
                city_input := View{visible: false width: Fill height: Fit input := Field{empty_text: "搜索家人所在城市"}}
                city_search := View{ width: Fill height: Fit city_search_btn := ActionButton{text: "搜索城市"} }
                city_status := View{visible: false width: Fill height: Fit status_text := Muted{text: "输入城市并搜索"}}
                city_results := View{ visible: false width: Fill height: Fit flow: Down spacing: 4
                    city0 := View{width: Fill height: Fit city_btn := ActionButton{text: ""}}
                    city1 := View{width: Fill height: Fit city_btn := ActionButton{text: ""}}
                    city2 := View{width: Fill height: Fit city_btn := ActionButton{text: ""}}
                    city3 := View{width: Fill height: Fit city_btn := ActionButton{text: ""}}
                    city4 := View{width: Fill height: Fit city_btn := ActionButton{text: ""}}
                }
                saved_cities := View{ visible: false width: Fill height: Fit flow: Down spacing: 4
                    saved0 := View{width: Fill height: Fit flow: Right spacing: 6 saved_btn := ActionButton{width: Fill text: ""} edit_btn := ActionButton{text: "资料"}}
                    saved1 := View{width: Fill height: Fit flow: Right spacing: 6 saved_btn := ActionButton{width: Fill text: ""} edit_btn := ActionButton{text: "资料"}}
                    saved2 := View{width: Fill height: Fit flow: Right spacing: 6 saved_btn := ActionButton{width: Fill text: ""} edit_btn := ActionButton{text: "资料"}}
                    saved3 := View{width: Fill height: Fit flow: Right spacing: 6 saved_btn := ActionButton{width: Fill text: ""} edit_btn := ActionButton{text: "资料"}}
                    saved4 := View{width: Fill height: Fit flow: Right spacing: 6 saved_btn := ActionButton{width: Fill text: ""} edit_btn := ActionButton{text: "资料"}}
                }
                care_fields := View{visible: false width: Fill height: Fit flow: Down spacing: 6
                    Label{width: Fill height: Fit text: "牵挂资料 · 仅用于起草关心消息" draw_text +: {color: #F3A44D text_style: theme.font_bold{font_size: 11.0}}}
                    care_name := Field{empty_text: "对方怎么称呼？例如：妈妈"}
                    care_relation := Field{empty_text: "关系，例如：妈妈、伴侣、朋友"}
                    care_room := Field{empty_text: "Rinx 会话名称或 Matrix 房间 ID"}
                    care_load_rooms := ActionButton{text: "从 Rinx 选择会话"}
                    care_room_hint := Muted{text: "会话列表仅显示 Rinx 当前账号已加入的房间"}
                    care_rooms := View{visible: false width: Fill height: Fit flow: Down spacing: 4
                        room0 := ActionButton{width: Fill text: ""}
                        room1 := ActionButton{width: Fill text: ""}
                        room2 := ActionButton{width: Fill text: ""}
                        room3 := ActionButton{width: Fill text: ""}
                        room4 := ActionButton{width: Fill text: ""}
                    }
                    care_details := Field{empty_text: "生活细节，例如：雨伞放在门口柜子里"}
                    care_tone := ActionButton{text: "语气：叮嘱 · 点击切换"}
                    care_alert := ActionButton{text: "没有待处理的天气提醒"}
                    care_message := Field{empty_text: "编辑要发送的牵挂消息"}
                    care_draft := ActionButton{text: "按当地天气起草"}
                    care_save := ActionButton{text: "保存牵挂资料"}
                    care_send := ActionButton{text: "发送到 Rinx 会话"}
                }
                close_editor := ActionButton{text: "完成"}
            }

            Card{
                View{ width: Fill height: Fit flow: Right align: Align{y: 0.5}
                    Title{ text: "我的日程" }
                    View{ width: Fill height: 1 }
                    agenda_count := Muted{ width: Fit }
                }
                Muted{ text: "AI 可结合安排与天气，帮你挑选合适的出行时间。" }
                i0 := AgendaRow{ visible: false }
                i1 := AgendaRow{ visible: false }
                i2 := AgendaRow{ visible: false }
                empty := Muted{ text: "今天还没有安排" }
            }
            Muted{ text: "天气助手不会要求你填写密码或密钥。健康提示只作天气参考，不代替医疗建议。" }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct MineScreen {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[rust(0usize)]
    editor: usize,
    #[rust]
    care_edit_index: Option<usize>,
    #[rust]
    care_rooms: Vec<(String, String)>,
    #[rust(String::from("叮嘱"))]
    care_tone: String,
}

#[derive(Clone, Default)]
pub struct WeatherProfile {
    pub wardrobe: String,
    pub factors: Vec<String>,
    pub family_cities: Vec<crate::model::City>,
    pub care_contacts: Vec<CareContact>,
    pub care_reminders: Vec<CareReminder>,
}

/// One city-level care recipient. Rinx resolves the exact room name or id
/// against the signed-in account before accepting a message request.
#[derive(Clone)]
pub struct CareContact {
    pub city: crate::model::City,
    pub name: String,
    pub relationship: String,
    pub matrix_room: String,
    pub personal_details: String,
    pub tone: String,
}

#[derive(Clone)]
pub struct CareReminder {
    pub contact_index: usize,
    pub event_key: String,
    pub date: String,
    pub created_at: String,
    pub fact: String,
    pub draft: String,
    pub sent: bool,
}

impl MineScreen {
    fn item(&self, cx: &mut Cx, i: usize) -> WidgetRef {
        self.view.widget(cx, &[LiveId::from_str(ITEMS[i.min(ITEMS.len() - 1)])])
    }

    pub fn render(&mut self, cx: &mut Cx, schedule: &ScheduleState) {
        let mut items = schedule.visible();
        items.retain(|item| !item.done);
        let count = items.len();
        self.view.label(cx, ids!(agenda_count)).set_text(cx, &format!("{count} 项"));
        self.view.widget(cx, ids!(empty)).set_visible(cx, count == 0);
        for i in 0..ITEMS.len() {
            let row = self.item(cx, i);
            let Some(item) = items.get(i) else {
                row.set_visible(cx, false);
                continue;
            };
            row.set_visible(cx, true);
            row.label(cx, ids!(at)).set_text(cx, &item.time);
            row.label(cx, ids!(title)).set_text(cx, &item.title);
            let place = if item.place.is_empty() { item.date.as_str() } else { item.place.as_str() };
            row.label(cx, ids!(place)).set_text(cx, place);
        }
    }

    pub fn open_editor(&mut self, cx: &mut Cx, which: usize, profile: &WeatherProfile) {
        self.editor = which;
        self.care_edit_index = None;
        let panel = self.view.widget(cx, ids!(editor));
        panel.set_visible(cx, true);
        for id in ["wardrobe_input", "wardrobe_save", "factors", "city_input", "city_search", "city_status", "city_results", "saved_cities", "care_fields"] {
            panel.widget(cx, &[LiveId::from_str(id)]).set_visible(cx, false);
        }
        let title = match which { 1 => "我的衣橱", 2 => "身体气象站", _ => "跨城牵挂" };
        panel.label(cx, ids!(editor_title)).set_text(cx, title);
        match which {
            1 => {
                panel.widget(cx, ids!(wardrobe_input)).set_visible(cx, true);
                panel.widget(cx, ids!(wardrobe_save)).set_visible(cx, true);
                panel.widget(cx, ids!(wardrobe_input)).text_input(cx, ids!(input)).set_text(cx, &profile.wardrobe);
            }
            2 => {
                panel.widget(cx, ids!(factors)).set_visible(cx, true);
                panel.widget(cx, ids!(factors)).widget(cx, ids!(custom_factor_entry)).set_visible(cx, false);
                self.render_factors(cx, profile);
            }
            _ => {
                panel.widget(cx, ids!(city_input)).set_visible(cx, true);
                panel.widget(cx, ids!(city_search)).set_visible(cx, true);
                panel.widget(cx, ids!(city_status)).set_visible(cx, true);
                panel.widget(cx, ids!(city_results)).set_visible(cx, true);
                panel.widget(cx, ids!(saved_cities)).set_visible(cx, true);
                let summary = if profile.family_cities.is_empty() { "搜索并添加家人所在城市".into() } else { format!("已添加：{}", profile.family_cities.iter().map(|c| c.name.as_ref()).collect::<Vec<_>>().join("、")) };
                panel.label(cx, ids!(city_status)).set_text(cx, &summary);
            }
        }
    }

    pub fn close_editor(&mut self, cx: &mut Cx) { self.view.widget(cx, ids!(editor)).set_visible(cx, false); self.editor = 0; }
    pub fn editor(&self) -> usize { self.editor }
    pub fn wardrobe_input(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).widget(cx, ids!(wardrobe_input)).text_input(cx, ids!(input)) }
    pub fn city_input(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).widget(cx, ids!(city_input)).text_input(cx, ids!(input)) }
    pub fn row_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        let hit = |button: ButtonRef| button.pressed(actions) || button.clicked(actions);
        if hit(self.view.widget(cx, ids!(wardrobe)).button(cx, ids!(click))) { return Some(1); }
        if hit(self.view.widget(cx, ids!(health)).button(cx, ids!(click))) { return Some(2); }
        if hit(self.view.widget(cx, ids!(care)).button(cx, ids!(click))) { return Some(3); }
        None
    }
    pub fn save_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { let b = self.view.widget(cx, ids!(editor)).widget(cx, ids!(wardrobe_save)).button(cx, ids!(save_btn)); b.pressed(actions) || b.clicked(actions) }
    pub fn close_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { let b = self.view.widget(cx, ids!(editor)).button(cx, ids!(close_editor)); b.pressed(actions) || b.clicked(actions) }
    pub fn search_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { let b = self.view.widget(cx, ids!(editor)).widget(cx, ids!(city_search)).button(cx, ids!(city_search_btn)); b.pressed(actions) || b.clicked(actions) }
    pub fn factor_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        let row = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).widget(cx, ids!(factor_row));
        for i in 0..PREF_FACTORS.len() {
            let id = LiveId::from_str(&format!("factor{i}"));
            let b = row.button(cx, &[id]);
            if b.pressed(actions) || b.clicked(actions) || row.view(cx, &[id]).finger_up(actions).is_some() { return Some(i); }
        }
        None
    }
    pub fn custom_factor_input(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).text_input(cx, ids!(custom_factor_input)) }
    pub fn add_custom_factor_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        let factors = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors));
        let b = factors.button(cx, ids!(custom_factor_add));
        b.pressed(actions) || b.clicked(actions) || factors.view(cx, ids!(custom_factor_add)).finger_up(actions).is_some()
    }
    pub fn custom_factor_plus_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        let factors = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).widget(cx, ids!(factor_row));
        let b = factors.button(cx, ids!(custom_factor_plus));
        b.pressed(actions) || b.clicked(actions) || factors.view(cx, ids!(custom_factor_plus)).finger_up(actions).is_some()
    }
    pub fn toggle_custom_factor_entry(&mut self, cx: &mut Cx) {
        let factors = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors));
        let entry = factors.widget(cx, ids!(custom_factor_entry));
        entry.set_visible(cx, !entry.visible());
    }
    pub fn hide_custom_factor_entry(&mut self, cx: &mut Cx) {
        self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).widget(cx, ids!(custom_factor_entry)).set_visible(cx, false);
    }
    pub fn set_custom_factor_status(&self, cx: &mut Cx, text: &str) { self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).label(cx, ids!(custom_factor_status)).set_text(cx, text); }
    pub fn custom_factor_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        let rows = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).widget(cx, ids!(custom_factors));
        for i in 0..5 {
            let id = LiveId::from_str(&format!("custom{i}"));
            let b = rows.button(cx, &[id]);
            if b.pressed(actions) || b.clicked(actions) || rows.view(cx, &[id]).finger_up(actions).is_some() { return Some(i); }
        }
        None
    }
    pub fn render_factors(&mut self, cx: &mut Cx, profile: &WeatherProfile) {
        let row = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).widget(cx, ids!(factor_row));
        for (i, factor) in PREF_FACTORS.iter().enumerate() {
            let selected = profile.factors.iter().any(|value| value == factor);
            row.button(cx, &[LiveId::from_str(&format!("factor{i}"))]).set_text(cx,
                &if selected { format!("✓ {factor}") } else { factor.to_string() });
        }
        let rows = self.view.widget(cx, ids!(editor)).widget(cx, ids!(factors)).widget(cx, ids!(custom_factors));
        let custom = profile.factors.iter().filter(|factor| !PREF_FACTORS.contains(&factor.as_str())).collect::<Vec<_>>();
        rows.set_visible(cx, !custom.is_empty());
        for i in 0..5 {
            let button = rows.button(cx, &[LiveId::from_str(&format!("custom{i}"))]);
            if let Some(label) = custom.get(i) { button.set_visible(cx, true); button.set_text(cx, &format!("× {}", label)); }
            else { button.set_visible(cx, false); }
        }
    }
    pub fn city_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..5 { let b = self.view.widget(cx, ids!(editor)).widget(cx, &[LiveId::from_str(&format!("city{i}"))]).button(cx, ids!(city_btn)); if b.pressed(actions) || b.clicked(actions) { return Some(i); } }
        None
    }
    pub fn saved_city_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..5 { let b = self.view.widget(cx, ids!(editor)).widget(cx, &[LiveId::from_str(&format!("saved{i}"))]).button(cx, ids!(saved_btn)); if b.pressed(actions) || b.clicked(actions) { return Some(i); } }
        None
    }
    pub fn edit_city_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..5 { let b = self.view.widget(cx, ids!(editor)).widget(cx, &[LiveId::from_str(&format!("saved{i}"))]).button(cx, ids!(edit_btn)); if b.pressed(actions) || b.clicked(actions) { return Some(i); } }
        None
    }
    pub fn open_care_fields(&mut self, cx: &mut Cx, index: usize, profile: &WeatherProfile) {
        self.care_edit_index = Some(index);
        let panel = self.view.widget(cx, ids!(editor));
        let Some(contact) = profile.care_contacts.get(index) else { return; };
        panel.widget(cx, ids!(care_fields)).set_visible(cx, true);
        self.care_tone = if ["叮嘱", "轻松", "简短"].contains(&contact.tone.as_str()) { contact.tone.clone() }
            else if ["妈妈", "母亲", "爸爸", "父亲", "爷爷", "奶奶", "外公", "外婆"].contains(&contact.relationship.as_str()) { "叮嘱".into() }
            else if ["伴侣", "爱人", "丈夫", "妻子"].contains(&contact.relationship.as_str()) { "轻松".into() }
            else { "简短".into() };
        self.render_care_tone(cx);
        panel.text_input(cx, ids!(care_name)).set_text(cx, &contact.name);
        panel.text_input(cx, ids!(care_relation)).set_text(cx, &contact.relationship);
        panel.text_input(cx, ids!(care_room)).set_text(cx, &contact.matrix_room);
        panel.text_input(cx, ids!(care_details)).set_text(cx, &contact.personal_details);
        panel.text_input(cx, ids!(care_message)).set_text(cx, "");
        let reminder = profile.care_reminders.iter().find(|r| r.contact_index == index && !r.sent);
        let alert = panel.button(cx, ids!(care_alert));
        if let Some(reminder) = reminder { alert.set_text(cx, &format!("{} · 点击写入草稿", reminder.fact)); }
        else { alert.set_text(cx, "没有待处理的天气提醒"); }
        panel.widget(cx, ids!(care_fields)).redraw(cx);
    }
    pub fn care_edit_index(&self) -> Option<usize> { self.care_edit_index }
    pub fn care_name(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).text_input(cx, ids!(care_name)) }
    pub fn care_relation(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).text_input(cx, ids!(care_relation)) }
    pub fn care_room(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).text_input(cx, ids!(care_room)) }
    pub fn care_details(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).text_input(cx, ids!(care_details)) }
    pub fn care_message(&self, cx: &mut Cx) -> TextInputRef { self.view.widget(cx, ids!(editor)).text_input(cx, ids!(care_message)) }
    pub fn care_tone(&self) -> &str { &self.care_tone }
    pub fn cycle_care_tone(&mut self, cx: &mut Cx) {
        self.care_tone = match self.care_tone.as_str() { "叮嘱" => "轻松", "轻松" => "简短", _ => "叮嘱" }.into();
        self.render_care_tone(cx);
    }
    fn render_care_tone(&mut self, cx: &mut Cx) {
        self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_tone))
            .set_text(cx, &format!("语气：{} · 点击切换", self.care_tone));
    }
    pub fn care_tone_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_tone)).clicked(actions) }
    pub fn care_load_rooms_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_load_rooms)).clicked(actions) }
    pub fn care_room_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..5 { let b = self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).widget(cx, ids!(care_rooms)).button(cx, &[LiveId::from_str(&format!("room{i}"))]); if b.pressed(actions) || b.clicked(actions) { return Some(i); } }
        None
    }
    pub fn selected_care_room(&self, index: usize) -> Option<&str> { self.care_rooms.get(index).map(|(_, name)| name.as_str()) }
    pub fn set_care_rooms(&mut self, cx: &mut Cx, rooms: Vec<(String, String)>, status: &str) {
        self.care_rooms = rooms;
        let panel = self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields));
        panel.label(cx, ids!(care_room_hint)).set_text(cx, status);
        let rows = panel.widget(cx, ids!(care_rooms));
        rows.set_visible(cx, !self.care_rooms.is_empty());
        for i in 0..5 {
            let row = rows.button(cx, &[LiveId::from_str(&format!("room{i}"))]);
            if let Some((id, name)) = self.care_rooms.get(i) { row.set_visible(cx, true); row.set_text(cx, &format!("{name} · {id}")); }
            else { row.set_visible(cx, false); }
        }
    }
    pub fn care_save_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_save)).clicked(actions) }
    pub fn care_send_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_send)).clicked(actions) }
    pub fn care_draft_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_draft)).clicked(actions) }
    pub fn care_alert_hit(&self, cx: &mut Cx, actions: &Actions) -> bool { self.view.widget(cx, ids!(editor)).widget(cx, ids!(care_fields)).button(cx, ids!(care_alert)).clicked(actions) }
    pub fn apply_care_alert(&self, cx: &mut Cx, profile: &WeatherProfile) -> bool {
        let Some(index) = self.care_edit_index else { return false; };
        let Some(reminder) = profile.care_reminders.iter().find(|r| r.contact_index == index && !r.sent) else { return false; };
        self.view.widget(cx, ids!(editor)).text_input(cx, ids!(care_message)).set_text(cx, &reminder.draft);
        true
    }
    pub fn render_profile(&mut self, cx: &mut Cx, profile: &WeatherProfile, results: &[crate::model::City], status: &str) {
        let care_detail = if profile.family_cities.is_empty() { "添加家人所在城市，开始天气监测".to_string() }
            else if !profile.care_reminders.iter().any(|r| !r.sent) { format!("{} 个家人城市 · 天气监测中", profile.family_cities.len()) }
            else { format!("{} 个家人城市 · {} 条待处理提醒", profile.family_cities.len(), profile.care_reminders.iter().filter(|r| !r.sent).count()) };
        for (id, text) in [("wardrobe", if profile.wardrobe.is_empty() { "添加常穿单品，获得具体搭配建议".to_string() } else { profile.wardrobe.clone() }), ("health", if profile.factors.is_empty() { "设置你想关注的天气因素".to_string() } else { profile.factors.join(" · ") }), ("care", care_detail)] {
            let row = self.view.widget(cx, &[LiveId::from_str(id)]);
            let body = row.widget(cx, ids!(body));
            body.label(cx, ids!(detail)).set_text(cx, &text);
            body.label(cx, ids!(status)).set_text(cx, if text.starts_with("添加") || text.starts_with("设置") { "待设置" } else { "已设置" });
        }
        if self.editor == 3 {
            let p = self.view.widget(cx, ids!(editor));
            p.widget(cx, ids!(city_status)).label(cx, ids!(status_text)).set_text(cx, status);
            for i in 0..5 { let row = p.widget(cx, &[LiveId::from_str(&format!("city{i}"))]); let b = row.button(cx, ids!(city_btn));
                if let Some(city) = results.get(i) { row.set_visible(cx, true); b.set_text(cx, &city.name); } else { row.set_visible(cx, false); }
            }
            for i in 0..5 { let row = p.widget(cx, &[LiveId::from_str(&format!("saved{i}"))]); let b = row.button(cx, ids!(saved_btn));
                if let Some(city) = profile.family_cities.get(i) { row.set_visible(cx, true); b.set_text(cx, &format!("查看天气 · {}", city.name)); } else { row.set_visible(cx, false); }
            }
            for i in 0..5 { let row = p.widget(cx, &[LiveId::from_str(&format!("saved{i}"))]);
                if let Some(contact) = profile.care_contacts.get(i) { row.button(cx, ids!(edit_btn)).set_text(cx, if contact.name.is_empty() { "设置资料" } else { "编辑资料" }); }
            }
            if let Some(index) = self.care_edit_index {
                let alert = profile.care_reminders.iter().find(|r| r.contact_index == index && !r.sent);
                p.widget(cx, ids!(care_fields)).button(cx, ids!(care_alert)).set_text(cx,
                    &alert.map(|r| format!("{} · 点击写入草稿", r.fact)).unwrap_or_else(|| "没有待处理的天气提醒".into()));
            }
        }
    }
}

impl Widget for MineScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}


// 地区切换页：从 Open-Meteo geocoding 搜到的结果里点一个，回天气页。
//
// 跟 WeatherScreen 不共享 DSL 类型（`script_mod!` 内部 let 不会跨模块暴露），
// 所以 PlaceRow / 搜索条 / 状态文字都各自再声明一份。状态都从 `WeatherState`
// 借——`place_results` / `place_status` / `place_flight` / `place_seq` 这些
// `WeatherState` 上的方法原样保留，Shell 来路由网络响应。
//
// 中文单字（"天" → 天津 / 天水 / 天门 / ...）就能开搜：weather::search_city 把
// 最低长度从 2 调到 1，geocoding_url 把 count 从 10 提到 100，所以这里要摆
// 足够多的预声明槽位，并且包在 ScrollYView 里让用户能滚。
use makepad_widgets::*;

use crate::ui;

/// 屏幕上的搜索结果槽。20 条够覆盖中文单字 + 国际城市的常见前缀组合。
const PLACE_SLOTS: [&str; 20] = [
    "p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9",
    "p10", "p11", "p12", "p13", "p14", "p15", "p16", "p17", "p18", "p19",
];

fn slot(names: &[&str], i: usize) -> LiveId {
    LiveId::from_str(names[i.min(names.len() - 1)])
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let Eyebrow = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #F3A44D text_style: theme.font_regular{font_size: 10.0} }
    }
    let TinyR = Label{
        padding: 0 margin: 0 width: Fill height: Fit max_lines: 1
        draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 9.0} }
    }
    let Sub = Label{
        padding: 0 margin: 0 max_lines: 2
        draw_text +: { color: #B8B4AC text_style: theme.font_regular{font_size: 10.5} }
    }
    let TitleLg = Label{
        padding: 0 margin: 0 width: Fit height: Fit
        draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 21.0} }
    }

    let BackBtn = RoundedShadowView{
        width: 34 height: 34
        padding: 0 margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #303030 border_radius: 17.0 border_size: 1.0 border_color: #414141 }
        glyph := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            text: "‹"
            draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 16.0} }
        }
    }
    let SearchBtn = RoundedShadowView{
        width: Fit height: 42
        padding: Inset{top: 0 left: 15 right: 15 bottom: 0}
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #F3A44D border_radius: 13.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            text: "搜索"
            draw_text +: { color: #251A0D text_style: theme.font_bold{font_size: 11.0} }
        }
    }

    // 单条搜索结果：左文字 + 右箭头。
    let PlaceRow = RoundedShadowView{
        width: Fill height: 48 flow: Right spacing: 10
        padding: Inset{left: 14 right: 14}
        align: Align{y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #252525 border_radius: 14.0 border_size: 1.0 border_color: #393939 }
        stack := View{
            width: Fill height: Fit flow: Down spacing: 3
            align: Align{y: 0.5}
            name := Label{ width: Fit height: Fit padding: 0 margin: 0
                draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 12.5} } }
            detail := TinyR{}
        }
        arrow := Label{ width: Fit height: Fit padding: 0 margin: 0 text: "›"
            draw_text +: { color: #F3A44D text_style: theme.font_bold{font_size: 17.0} } }
    }

    mod.widgets.PlacePickerScreenBase = #(PlacePickerScreen::register_widget(vm))
    mod.widgets.PlacePickerScreen = set_type_default() do mod.widgets.PlacePickerScreenBase{
        width: Fill height: Fill
        flow: Down spacing: 14
        padding: Inset{top: 15 left: 14 right: 14 bottom: 14}
        show_bg: true
        draw_bg +: { color: #191919 }

        // 顶栏：返回 + 标题。
        View{ width: Fill height: Fit flow: Right spacing: 14 align: Align{y: 0.5}
            back := BackBtn{}
            View{ width: Fit height: Fit flow: Down spacing: 3
                Eyebrow{ text: "PLACE · 选择地区" }
                TitleLg{ text: "切换到哪个区县？" }
            }
            View{ width: Fill height: 1 }
        }

        // 搜索行：输入 + 按钮。
        View{ width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
            place_input := TextInput{
                width: Fill height: 42
                empty_text: "输入地区名，如：天 / 天津 / 东丽区"
                draw_bg +: {
                    color: #303030 color_empty: #303030 color_hover: #303030
                    color_focus: #303030 color_down: #303030
                    border_radius: 13.0
                }
                draw_text +: {
                    color: #F2EEE6 color_focus: #F2EEE6
                    color_empty: #AAA69E color_empty_focus: #AAA69E
                    text_style: theme.font_regular{font_size: 11.5}
                }
            }
            place_search := SearchBtn{}
        }

        // 状态文字：搜索中 / 找到 N 个 / 没找到 / 网络失败。
        place_status := TinyR{}

        // 结果列表：20 个预声明槽位，按索引显示。外层 ScrollYView 让 20 条以内
        // 不至于撑爆窗口。
        ScrollYView{
            width: Fill height: Fill flow: Down spacing: 6
            show_bg: true
            draw_bg +: { color: #191919 }
            scroll_bars +: { show_scroll_x: false }
            padding: Inset{top: 4}

            p0 := PlaceRow{ visible: false }
            p1 := PlaceRow{ visible: false }
            p2 := PlaceRow{ visible: false }
            p3 := PlaceRow{ visible: false }
            p4 := PlaceRow{ visible: false }
            p5 := PlaceRow{ visible: false }
            p6 := PlaceRow{ visible: false }
            p7 := PlaceRow{ visible: false }
            p8 := PlaceRow{ visible: false }
            p9 := PlaceRow{ visible: false }
            p10 := PlaceRow{ visible: false }
            p11 := PlaceRow{ visible: false }
            p12 := PlaceRow{ visible: false }
            p13 := PlaceRow{ visible: false }
            p14 := PlaceRow{ visible: false }
            p15 := PlaceRow{ visible: false }
            p16 := PlaceRow{ visible: false }
            p17 := PlaceRow{ visible: false }
            p18 := PlaceRow{ visible: false }
            p19 := PlaceRow{ visible: false }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct PlacePickerScreen {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
}

impl PlacePickerScreen {
    fn row(&self, cx: &mut Cx, i: usize) -> WidgetRef {
        self.view.widget(cx, &[slot(&PLACE_SLOTS, i)])
    }

    pub fn search_input(&self, cx: &mut Cx) -> TextInputRef {
        self.view.text_input(cx, ids!(place_input))
    }

    pub fn back_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        self.view.view(cx, ids!(back)).finger_up(actions).is_some()
    }

    pub fn search_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        self.view.view(cx, ids!(place_search)).finger_up(actions).is_some()
    }

    pub fn place_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..PLACE_SLOTS.len() {
            if self.view.view(cx, &[slot(&PLACE_SLOTS, i)]).finger_up(actions).is_some() {
                return Some(i);
            }
        }
        None
    }

    /// 按 `place_results` 刷全部槽位 + 状态文字。
    pub fn render(&mut self, cx: &mut Cx, st: &crate::weather::WeatherState) {
        let status = self.view.label(cx, ids!(place_status));
        status.set_text(cx, &st.place_status);
        ui::tint(
            &status,
            cx,
            if st.place_status.contains("失败") || st.place_status.contains("没有找到") {
                vec4(0.851, 0.322, 0.318, 1.0)
            } else {
            vec4(0.667, 0.651, 0.620, 1.0)
            },
        );

        // 搜索按钮的颜色保持不变——再点一次会取消旧请求再开新的。
        ui::fill(
            &self.view.widget(cx, ids!(place_search)),
            cx,
            vec4(0.953, 0.643, 0.294, 1.0),
        );
        ui::tint(
            &self.view.widget(cx, ids!(place_search)).label(cx, &[live_id!(name)]),
            cx,
            vec4(0.145, 0.098, 0.043, 1.0),
        );
        ui::fill(
            &self.view.widget(cx, ids!(back)),
            cx,
            vec4(0.188, 0.188, 0.188, 1.0),
        );

        for i in 0..PLACE_SLOTS.len() {
            let row = self.row(cx, i);
            let Some(place) = st.place_results.get(i) else {
                row.set_visible(cx, false);
                continue;
            };
            row.set_visible(cx, true);
            row.label(cx, &[live_id!(name)]).set_text(cx, place.name.as_ref());
            let detail = row.label(cx, &[live_id!(detail)]);
            detail.set_text(cx, place.en.as_ref());
            ui::tint(&detail, cx, vec4(0.667, 0.651, 0.620, 1.0));
        }
        self.view.redraw(cx);
    }
}

impl Widget for PlacePickerScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope)
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

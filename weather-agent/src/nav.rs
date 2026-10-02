// 底部导航栏：天气 / 控制台 / 日程，浅色调。
//
// 只管显示和上报点击；当前在哪一页由 shell 决定。选中态换颜色、点亮顶部
// 那条小指示条。
use makepad_widgets::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NavHit {
    Weather,
    Sky,
    Console,
    Me,
}

impl NavHit {
    pub const ALL: [Self; 4] = [Self::Weather, Self::Sky, Self::Console, Self::Me];
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // 一个入口：图标在上、标签在下，选中态由 Rust 改颜色。
    let NavItem = View{
        width: Fill
        height: Fill
        flow: Down
        align: Align{x: 0.5 y: 0.5}
        show_bg: true
        // `cursor` 不只是手型：`View::handle_event` 里 `cursor.is_some()` 是命中测试的开关，
        // 不设它这个 View 就不会捕获鼠标按下，也就永远不会收到 `FingerUp`。
        cursor: MouseCursor.Hand
        draw_bg +: { color: #00000000 }

        indicator := View{
            width: 24
            height: 3
            show_bg: true
            visible: false
            draw_bg +: { color: #3D7BFA border_radius: 1.5 }
        }
        icon := Label{
            width: Fill
            height: 22
            padding: 0
            margin: Inset{top: 1 left: 0 right: 0 bottom: 0}
            align: Align{x: 0.5 y: 0.5}
            text: "🌤"
            draw_text +: {
                color: #8B95A6
                text_style: theme.font_regular{font_size: 19}
            }
        }
        label := Label{
            width: Fill
            height: 13
            padding: 0
            margin: Inset{top: 2 left: 0 right: 0 bottom: 0}
            align: Align{x: 0.5 y: 0.5}
            text: "天气"
            draw_text +: {
                color: #8B95A6
                text_style: theme.font_regular{font_size: 10}
            }
        }
    }

    mod.widgets.NavBarBase = #(NavBar::register_widget(vm))
    mod.widgets.NavBar = set_type_default() do mod.widgets.NavBarBase{
        width: Fill
        height: 58
        flow: Down
        spacing: 0
        show_bg: true
        draw_bg +: { color: #202020 }

        // 这个 fork 的 DrawColor 没有 border 那套属性，只能自己画一条细线跟纸面分开。
        hairline := View{
            width: Fill
            height: 1
            show_bg: true
            draw_bg +: { color: #353535 }
        }
        items := View{
            width: Fill
            height: Fill
            flow: Right
            spacing: 0
            nav_weather := NavItem{ icon.text: "☀" label.text: "今天" }
            nav_sky := NavItem{ icon.text: "🎨" label.text: "天空" }
            nav_console := NavItem{ icon.text: "💬" label.text: "问答" }
            nav_me := NavItem{ icon.text: "♡" label.text: "我的" }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct NavBar {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
}

impl NavBar {
    /// 入口的槽位 id。注意用 `live_id!`（单个 LiveId），不是 `ids!`（切片）。
    fn id(hit: NavHit) -> LiveId {
        match hit {
            NavHit::Weather => live_id!(nav_weather),
            NavHit::Sky => live_id!(nav_sky),
            NavHit::Console => live_id!(nav_console),
            NavHit::Me => live_id!(nav_me),
        }
    }

    /// 入口下的某个子部件。
    fn part(&self, cx: &mut Cx, hit: NavHit, name: LiveId) -> WidgetRef {
        self.view.widget(cx, &[Self::id(hit), name])
    }

    /// 有没有点中某个入口。
    pub fn hit(&self, cx: &mut Cx, actions: &Actions) -> Option<NavHit> {
        for hit in NavHit::ALL {
            if self.view.view(cx, &[Self::id(hit)]).finger_up(actions).is_some() {
                return Some(hit);
            }
        }
        None
    }

    /// 刷一次选中态：选中用主题蓝，其余用浅灰；选中才亮那条指示条。
    /// `script_apply_eval!` 要 `&mut` 接收者，所以先绑到可变局部。
    pub fn paint(&self, cx: &mut Cx, active: NavHit) {
        for hit in NavHit::ALL {
            let sel = hit == active;
            let ink = if sel { vec4(0.953, 0.643, 0.294, 1.0) } else { vec4(0.667, 0.651, 0.620, 1.0) };
            let mut icon = self.part(cx, hit, live_id!(icon));
            script_apply_eval!(cx, icon, { draw_text.color: #(ink) });
            let mut label = self.part(cx, hit, live_id!(label));
            script_apply_eval!(cx, label, { draw_text.color: #(ink) });
            self.part(cx, hit, live_id!(indicator)).set_visible(cx, sel);
        }
    }
}

impl Widget for NavBar {
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
    fn the_four_hits_are_distinct() {
        let all = NavHit::ALL;
        assert_eq!(all.len(), 4);
        assert_ne!(all[0], all[1]);
        assert_ne!(all[1], all[2]);
        assert_ne!(all[0], all[2]);
    }
}

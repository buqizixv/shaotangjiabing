// 控制台页：和内置 agent 对话。
//
// agent 本体在 `agent.rs`，这一页只管显示和输入。聊天记录放在 `ChatLog` 里、
// 由 shell 持有，换页签不会把对话冲掉。
//
// 对话历史用常驻文本行显示，用户与助手各用一种文字颜色和对齐方式。
use makepad_widgets::*;

use crate::model::{ChatEntry, Who};

const MAX_MESSAGES: usize = 10;
const MESSAGE_SLOTS: [&str; MAX_MESSAGES] = ["m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9"];

/// 一句话就问出去；点了就当成用户输入直接发。
pub const QUICK_PROMPTS: [&str; 4] = [
    "今天出门要带伞吗？",
    "现在冷不冷，穿什么合适？",
    "未来一周哪天最适合出行？",
    "明天天气怎么样？",
];
const QUICK_SLOTS: [&str; 4] = ["q0", "q1", "q2", "q3"];

/// 第 `i` 个快捷提问，越界就兜到最后一个。
pub fn quick_prompt(i: usize) -> String {
    QUICK_PROMPTS[i.min(QUICK_PROMPTS.len() - 1)].to_string()
}

/// 第 `i` 个槽位的 LiveId，越界兜到最后一个。
fn slot(names: &[&str], i: usize) -> LiveId {
    LiveId::from_str(names[i.min(names.len() - 1)])
}

/// agent 这一轮走到哪一步，状态行照着它换字。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    /// 模型在调 `weather.current`，真实数据还没回来。
    Tool,
    /// 正在往屏幕上写回答。
    Streaming,
}

/// 一段对话。`streaming` 是还没答完的那半截，答完了并入 `entries`。
#[derive(Clone, Debug, Default)]
pub struct ChatLog {
    entries: Vec<ChatEntry>,
    streaming: String,
    phase: Phase,
    /// 最近一次失败的原因，状态行留着给人看。
    failed: Option<String>,
}

impl ChatLog {
    pub fn new() -> Self {
        let mut log = Self::default();
        log.entries.push(ChatEntry::new(Who::Agent, "", "你好，我是知时。想问出行、穿搭，还是接下来几小时的天气？"));
        log
    }

    pub fn entries(&self) -> &[ChatEntry] {
        &self.entries
    }

    pub fn streaming(&self) -> &str {
        &self.streaming
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// 记一句用户的话，并切到「正在取数据」。
    pub fn push_user(&mut self, text: &str) {
        self.entries.push(ChatEntry::new(Who::User, "你", text));
        self.phase = Phase::Tool;
        self.failed = None;
    }

    pub fn push_note(&mut self, text: impl Into<String>) {
        self.entries.push(ChatEntry::new(Who::Note, "状态", text));
    }

    /// 流式收到一小段。
    pub fn append_stream(&mut self, delta: &str) {
        self.phase = Phase::Streaming;
        self.streaming.push_str(delta);
    }

    /// Replace the accumulated text when the host sends an authoritative
    /// partial transcript rather than a delta.
    pub fn set_stream(&mut self, text: &str) {
        self.phase = Phase::Streaming;
        self.streaming.clear();
        self.streaming.push_str(text);
    }

    /// 这一轮答完了：把攒着的半截收进正式记录。空串不入账。
    pub fn finish_stream(&mut self) {
        self.phase = Phase::Idle;
        let text = std::mem::take(&mut self.streaming);
        if !text.trim().is_empty() {
            self.entries.push(answer_card(&text));
        }
    }

    pub fn apply_choice(&mut self, entry_index: usize, choice_index: usize) -> Option<(String, bool, Option<crate::model::ScheduleProposal>)> {
        let entry = self.entries.get_mut(entry_index)?;
        let choice = entry.choices.get(choice_index)?.clone();
        let is_approval = entry.is_approval();
        let proposal = entry.schedule_proposal.clone();
        if is_approval {
            let approved = matches!(choice.as_str(), "同意" | "允许" | "确认" | "添加");
            entry.approval_decision = Some(approved);
            entry.choices.clear();
            entry.text.push_str(if approved { "\n\n已同意" } else { "\n\n已拒绝" });
        }
        Some((choice, is_approval, proposal))
    }

    /// 失败了：有半截就照单收进记录，再补一条报错提示。
    pub fn fail(&mut self, msg: &str) {
        self.phase = Phase::Idle;
        let text = std::mem::take(&mut self.streaming);
        if !text.trim().is_empty() {
            self.entries.push(ChatEntry::new(Who::Agent, "天气助手", text));
        }
        let note = if msg.trim().is_empty() { "出错了，再试一次" } else { msg };
        self.entries.push(ChatEntry::new(Who::Note, "提示", format!("（{note}）")));
        self.failed = Some(note.to_string());
    }

    /// 用户主动打断，已收到的半截照样留档。
    pub fn abort(&mut self) {
        self.phase = Phase::Idle;
        let text = std::mem::take(&mut self.streaming);
        if !text.trim().is_empty() {
            self.entries.push(ChatEntry::new(Who::Agent, "天气助手", text));
        }
    }

    /// 状态行显示什么。
    pub fn status_text(&self) -> String {
        match self.phase {
            Phase::Tool => "正在取天气数据…".into(),
            Phase::Streaming => "正在回答…".into(),
            Phase::Idle => match &self.failed {
                Some(e) => format!("上次失败：{e}"),
                None => "在线".into(),
            },
        }
    }
}

fn answer_card(raw: &str) -> ChatEntry {
    let text = raw.trim();
    let json = text.strip_prefix("```json").and_then(|s| s.strip_suffix("```"))
        .or_else(|| text.strip_prefix("```").and_then(|s| s.strip_suffix("```")))
        .unwrap_or(text).trim();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return ChatEntry::new(Who::Agent, "天气助手", text);
    };
    let body = value.get("body").and_then(serde_json::Value::as_str).unwrap_or(text);
    let proposal = value.get("schedule").filter(|v| v.is_object()).and_then(|v| {
        Some(crate::model::ScheduleProposal {
            title: v.get("title")?.as_str()?.trim().to_string(),
            date: v.get("date")?.as_str()?.trim().to_string(),
            time: v.get("time")?.as_str()?.trim().to_string(),
            place: v.get("place").and_then(serde_json::Value::as_str).unwrap_or("").trim().to_string(),
        })
    }).filter(|p| !p.title.is_empty());
    let mut entry = ChatEntry::new(
        Who::Agent,
        if proposal.is_some() { "日程权限申请" } else { value.get("title").and_then(serde_json::Value::as_str).unwrap_or("天气助手") },
        body,
    );
    entry.choices = value.get("choices").and_then(serde_json::Value::as_array).map(|items| {
        items.iter().filter_map(serde_json::Value::as_str).take(3).map(str::to_string).collect()
    }).unwrap_or_default();
    if proposal.is_some() && entry.choices.is_empty() {
        entry.choices = vec!["同意".into(), "拒绝".into()];
    }
    entry.schedule_proposal = proposal;
    entry
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    let CardTitle = Label{
        padding: 0 margin: 0
        draw_text +: { color: #F6F2EA text_style: theme.font_bold{font_size: 13.0} }
    }
    let Sub = Label{
        padding: 0 margin: 0 max_lines: 2
        draw_text +: { color: #B8B4AC text_style: theme.font_regular{font_size: 10.5} }
    }
    let Tiny = Label{
        padding: 0 margin: 0
        draw_text +: { color: #AAA69E text_style: theme.font_regular{font_size: 9.0} }
    }

    let MessageText = Label{
        width: Fill height: Fit max_lines: 0 padding: 0 margin: Inset{top: 5 bottom: 5}
        draw_text +: { color: #25292D text_style: theme.font_regular{font_size: 12.5} }
    }

    // 卡片底色，头部和输入条共用。
    let CardFlat = RoundedShadowView{
        width: Fill height: Fit flow: Down spacing: 0
        padding: Inset{top: 11 left: 12 right: 12 bottom: 11}
        show_bg: true
        draw_bg +: {
            color: #242424
            border_radius: 16.0
            border_size: 0.6
            border_color: #393939
            shadow_color: #00000020
            shadow_radius: 8.0
        }
    }

    // 快捷提问胶囊。
    let QuickChip = View{
        width: Fit height: 28
        padding: Inset{top: 0 left: 10 right: 10 bottom: 0}
        margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #343434 border_radius: 14.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            draw_text +: { color: #E1DDD5 text_style: theme.font_regular{font_size: 10.0} }
        }
    }

    // 方形动作按钮（发送 / 停止）。
    let ActionBtn = View{
        width: Fit height: 34
        padding: Inset{top: 0 left: 12 right: 12 bottom: 0}
        margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #F3A44D border_radius: 17.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            draw_text +: { color: #251A0D text_style: theme.font_bold{font_size: 11.0} }
        }
    }
    // 停止键是灰底变体。拆成独立类型是因为这个 fork 的 DSL 不吃 `name.draw_text +:`
    // 那种二级路径的 `+:`（会连带把整个 stop 块解析坏），颜色只能在类型里定死。
    let StopBtn = View{
        width: Fit height: 34
        padding: Inset{top: 0 left: 12 right: 12 bottom: 0}
        margin: 0
        align: Align{x: 0.5 y: 0.5}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: { color: #3A3A3A border_radius: 17.0 }
        name := Label{
            padding: 0 margin: 0 width: Fit height: Fit
            draw_text +: { color: #E1DDD5 text_style: theme.font_bold{font_size: 11.0} }
        }
    }

    mod.widgets.ConsoleScreenBase = #(ConsoleScreen::register_widget(vm))
    mod.widgets.ConsoleScreen = set_type_default() do mod.widgets.ConsoleScreenBase{
        width: Fill height: Fill
        flow: Down
        spacing: 0
        show_bg: true
        draw_bg +: { color: #F1F4F4 }

        View{ width: Fill height: Fit flow: Down spacing: 3 padding: Inset{top: 14 left: 16 right: 16 bottom: 8}
            Label{ width: Fill height: Fit padding: 0 margin: 0 text: "问问天气"
                draw_text +: { color: #302E2A text_style: theme.font_bold{font_size: 18.0} } }
            status := Label{ width: Fill height: Fit padding: 0 margin: 0
                draw_text +: { color: #827D76 text_style: theme.font_regular{font_size: 10.0} } }
        }
        messages := ScrollYView{
            width: Fill height: Fill flow: Down spacing: 9
            padding: Inset{top: 8 left: 16 right: 16 bottom: 10}
            show_bg: true
            draw_bg +: { color: #F1F4F4 }
            scroll_bars +: { show_scroll_x: false }
            choice_row := View{ width: Fill height: Fit flow: Right spacing: 6 visible: false
                c0 := Button{ width: Fit height: 30 text: "" }
                c1 := Button{ width: Fit height: 30 text: "" }
                c2 := Button{ width: Fit height: 30 text: "" }
            }
            m0 := MessageText{}
            m1 := MessageText{}
            m2 := MessageText{}
            m3 := MessageText{}
            m4 := MessageText{}
            m5 := MessageText{}
            m6 := MessageText{}
            m7 := MessageText{}
            m8 := MessageText{}
            m9 := MessageText{}
            Tiny{ width: Fill margin: Inset{top: 3} text: "试试这样问：" visible: false }
            View{ width: Fill height: Fit flow: Right spacing: 6 visible: false
                q0 := QuickChip{ name.text: "要带伞吗？" }
                q1 := QuickChip{ name.text: "穿什么合适？" }
                q2 := QuickChip{ name.text: "适合出行吗？" }
                q3 := QuickChip{ name.text: "明天如何？" }
            }
        }

        composer := CardFlat{
            width: Fill height: Fit
            flow: Right
            align: Align{y: 0.5}
            margin: Inset{left: 14 right: 14 bottom: 12}
            padding: Inset{top: 6 left: 10 right: 7 bottom: 6}
            draw_bg +: { color: #E9E6E1 border_radius: 30.0 border_size: 0.8 border_color: #D6D0C8 shadow_radius: 3.0 }
            input := TextInput{
                width: Fill height: 42
                empty_text: "问问天气，比如「几点跑步最好」..."
                draw_bg +: { color: #E9E6E1 color_empty: #E9E6E1 color_hover: #E9E6E1 color_focus: #E9E6E1 color_down: #E9E6E1 border_radius: 22.0 }
                draw_text +: {
                    color: #302E2A
                    color_focus: #302E2A
                    color_empty: #827D76
                    color_empty_focus: #827D76
                    text_style: theme.font_regular{font_size: 12.0}
                }
            }
            send := Button{
                width: 40 height: 40 text: "↑"
                draw_bg +: { color: #F3A44D color_hover: #F6B36D color_down: #E89136 border_size: 0.0 border_radius: 20.0 }
                draw_text +: { color: #251A0D text_style: theme.font_bold{font_size: 19.0} }
            }
            stop := Button{
                width: 40 height: 40 text: "■" visible: false
                draw_bg +: { color: #F3A44D color_hover: #F6B36D color_down: #E89136 border_size: 0.0 border_radius: 20.0 }
                draw_text +: { color: #251A0D text_style: theme.font_bold{font_size: 11.0} }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ConsoleScreen {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
}

impl ConsoleScreen {
    fn message(&self, cx: &mut Cx, i: usize) -> LabelRef {
        self.view.widget(cx, ids!(messages)).label(cx, &[slot(&MESSAGE_SLOTS, i)])
    }

    fn composer(&self, cx: &mut Cx) -> WidgetRef {
        self.view.widget(cx, ids!(composer))
    }

    /// 输入框。
    pub fn input(&self, cx: &mut Cx) -> TextInputRef {
        self.composer(cx).text_input(cx, ids!(input))
    }

    pub fn show_event_debug(&self, cx: &mut Cx, text: &str) {
        self.view.label(cx, ids!(status)).set_text(cx, text);
    }

    /// 发送按钮按下时立即提交，兼容某些窗口环境没有派发抬起事件的情况。
    pub fn send_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        // Resolve the button from its immediate parent. The previous lookup
        // walked the full path from the page root and could bind a stale
        // widget handle in the hosted module, so Pressed was never matched.
        let button = self.composer(cx).button(cx, ids!(send));
        button.pressed(actions) || button.clicked(actions)
    }

    /// 点没点停止。
    pub fn stop_hit(&self, cx: &mut Cx, actions: &Actions) -> bool {
        self.composer(cx).button(cx, ids!(stop)).clicked(actions)
    }

    /// 点中了第几个快捷提问。
    pub fn quick_hit(&self, cx: &mut Cx, actions: &Actions) -> Option<usize> {
        for i in 0..QUICK_PROMPTS.len() {
            if self.view.widget(cx, ids!(messages)).widget(cx, &[slot(&QUICK_SLOTS, i)])
                .as_view().finger_up(actions).is_some() {
                return Some(i);
            }
        }
        None
    }

    fn paint_message(&self, cx: &mut Cx, i: usize, entry: &ChatEntry) {
        let label = self.message(cx, i);
        let (speaker, color, align_x) = match entry.who {
            Who::User => ("你", vec4(0.62, 0.34, 0.10, 1.0), 1.0),
            Who::Agent => ("知时", vec4(0.12, 0.38, 0.52, 1.0), 0.0),
            Who::Note => ("提示", vec4(0.39, 0.42, 0.45, 1.0), 0.5),
        };
        let mut text = format!("{speaker}：\n{}", entry.text);
        if !entry.choices.is_empty() && entry.approval_decision.is_none() {
            text.push_str("\n可选：");
            text.push_str(&entry.choices.join(" / "));
        }
        label.set_text(cx, &text);
        label.set_text_color(cx, color);
        if let Some(mut inner) = label.borrow_mut() {
            inner.align = Align { x: align_x, y: 0.0 };
        };
    }

    /// 整页按 `log` 刷一遍。
    pub fn render(&mut self, cx: &mut Cx, log: &ChatLog, busy: bool, _model: &str) {
        // 状态行
        let status = self.view.label(cx, ids!(status));
        crate::ui::set_label(
            &status,
            cx,
            &log.status_text(),
            match log.phase() {
                Phase::Idle => vec4(0.667, 0.651, 0.620, 1.0),
                _ => vec4(0.953, 0.643, 0.294, 1.0),
            },
        );

        // Newest first: m0 stays at the top of the viewport, so new sends and
        // streamed replies never depend on a guessed scroll range.
        let mut shown = 0usize;
        if !log.streaming().trim().is_empty() && shown < MAX_MESSAGES {
            let live = ChatEntry::new(Who::Agent, "知时", log.streaming());
            self.paint_message(cx, shown, &live);
            shown += 1;
        } else if busy && log.entries().last().is_some_and(|entry| entry.who == Who::User) && shown < MAX_MESSAGES {
            let waiting = ChatEntry::new(Who::Agent, "知时", "正在结合天气信息整理回答…");
            self.paint_message(cx, shown, &waiting);
            shown += 1;
        }
        for entry in log.entries().iter().rev().take(MAX_MESSAGES.saturating_sub(shown)) {
            self.paint_message(cx, shown, entry);
            shown += 1;
        }
        for i in shown..MAX_MESSAGES {
            self.message(cx, i).set_text(cx, "");
        }

        let choices = log.entries().iter().enumerate().rev()
            .find(|(_, entry)| !entry.choices.is_empty() && entry.approval_decision.is_none());
        let row = self.view.widget(cx, ids!(messages)).view(cx, ids!(choice_row));
        row.set_visible(cx, choices.is_some());
        for i in 0..3 {
            let button = row.button(cx, &[slot(&["c0", "c1", "c2"], i)]);
            if let Some((_, entry)) = choices {
                button.set_visible(cx, entry.choices.get(i).is_some());
                if let Some(choice) = entry.choices.get(i) {
                    button.set_text(cx, choice);
                }
            } else {
                button.set_visible(cx, false);
            }
        }
        self.view.widget(cx, ids!(messages)).set_scroll_pos(cx, dvec2(0.0, 0.0));

        // 忙的时候把发送换成停止
        self.view.button(cx, &[live_id!(composer), live_id!(send)]).set_visible(cx, !busy);
        self.view.button(cx, &[live_id!(composer), live_id!(stop)]).set_visible(cx, busy);
        // Refresh after the messages and state controls change.
        self.view.redraw(cx);
    }

    pub fn choice_hit(&self, cx: &mut Cx, actions: &Actions, log: &ChatLog) -> Option<(usize, usize)> {
        let stream_offset = usize::from(!log.streaming().trim().is_empty() || self.busy_placeholder_needed(log));
        for (bubble_index, (entry_index, entry)) in log.entries().iter().enumerate().rev().enumerate() {
            let slot_index = stream_offset + bubble_index;
            if slot_index >= MAX_MESSAGES { break; }
            for choice_index in 0..entry.choices.len().min(3) {
                let row = self.view.widget(cx, ids!(messages)).view(cx, ids!(choice_row));
                if row.button(cx, &[slot(&["c0", "c1", "c2"], choice_index)]).clicked(actions)
                    && entry.approval_decision.is_none() {
                    return Some((entry_index, choice_index));
                }
            }
        }
        None
    }

    fn busy_placeholder_needed(&self, log: &ChatLog) -> bool {
        log.streaming().trim().is_empty() && log.entries().last().is_some_and(|entry| entry.who == Who::User)
    }
}

impl Widget for ConsoleScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_starts_with_a_welcome_note() {
        let log = ChatLog::new();
        assert_eq!(log.entries().len(), 1);
        assert!(matches!(log.entries()[0].who, Who::Agent));
        assert!(log.status_text().contains("在线"));
    }

    #[test]
    fn the_log_tracks_tool_then_stream() {
        let mut log = ChatLog::new();
        log.push_user("冷不冷");
        assert_eq!(log.entries().len(), 2);
        assert!(matches!(log.entries()[1].who, Who::User));
        assert_eq!(log.phase(), Phase::Tool);
        assert!(log.status_text().contains("取天气数据"));

        log.append_stream("现在 ");
        log.append_stream("26 度");
        assert_eq!(log.streaming(), "现在 26 度");
        assert_eq!(log.phase(), Phase::Streaming);
        assert!(log.status_text().contains("正在回答"));
    }

    #[test]
    fn finishing_folds_the_stream_back_in() {
        let mut log = ChatLog::new();
        log.push_user("x");
        log.append_stream("回答");
        log.finish_stream();
        assert!(log.streaming().is_empty());
        assert_eq!(log.entries().len(), 3);
        assert!(matches!(log.entries()[2].who, Who::Agent));
        assert_eq!(log.entries()[2].text, "回答");
        assert_eq!(log.phase(), Phase::Idle);
    }

    #[test]
    fn a_blank_stream_leaves_no_empty_bubble() {
        let mut log = ChatLog::new();
        log.push_user("x");
        log.append_stream("   ");
        log.finish_stream();
        assert_eq!(log.entries().len(), 2);
    }

    #[test]
    fn failure_keeps_partial_text_and_records_the_error() {
        let mut log = ChatLog::new();
        log.push_user("x");
        log.append_stream("半截");
        log.fail("网络超时");
        assert_eq!(log.entries().len(), 4);
        assert!(matches!(log.entries()[2].who, Who::Agent));
        assert!(matches!(log.entries()[3].who, Who::Note));
        assert!(log.entries()[3].text.contains("网络超时"));
        assert!(log.status_text().contains("网络超时"));
    }

    #[test]
    fn abort_leaves_partial_text_in_the_record() {
        let mut log = ChatLog::new();
        log.push_user("x");
        log.append_stream("半截");
        log.abort();
        assert!(log.streaming().is_empty());
        assert_eq!(log.entries().len(), 3);
        assert!(matches!(log.entries()[2].who, Who::Agent));
        assert_eq!(log.phase(), Phase::Idle);
    }

    #[test]
    fn the_quick_slot_lookup_wraps() {
        assert_eq!(slot(&QUICK_SLOTS, 9), LiveId::from_str("q3"));
    }

    #[test]
    fn the_quick_prompts_are_weather_questions() {
        assert_eq!(QUICK_PROMPTS.len(), 4);
        for p in QUICK_PROMPTS {
            assert!(!p.trim().is_empty());
        }
    }

}

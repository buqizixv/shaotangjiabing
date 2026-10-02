// weather-agent 独立窗格：浅色调，底部三页（天气 / 控制台 / 日程）。
//
// 窗格里只有一个 `Shell`，三页的状态和 agent 都在它身上；换页签只切显隐，
// 所以控制台里那句没答完的话，翻到天气页看一眼再回来还在流。
//
// 独立跑时，天气和日程这两个只读工具挂在窗自己的 AI 端口上（F10 面板）；
// 同一个 manifest 在托管时由 src/module.rs 交给宿主。

use makepad_ai_services::port::{AiServicePort, PortEvent};
use octosense_weather_agent::ai;
use octosense_weather_agent::shell;
pub use makepad_widgets;
use makepad_widgets::*;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.title: "天气助手"
                window.inner_size: vec2(430, 860)
                pass +: { clear_color: #F5F7FA }
                body +: {
                    padding: 0
                    margin: 0
                    spacing: 0
                    shell := Shell{}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    /// 窗的 AI 服务：托管时是壳的总线，独立时是窗自己的 F10 面板。
    #[rust]
    ai_port: Option<AiServicePort>,
    /// 上次发出去的上下文摘要；没变就别再打扰总线。
    #[rust]
    ai_context: String,
}

impl App {
    fn shell(&self, cx: &mut Cx) -> WidgetRef {
        self.ui.widget(cx, ids!(shell))
    }

    /// 给总线的易失上下文：预报摘要和日程一起，一眼看清这窗里有什么。
    fn ai_summary(&self, cx: &mut Cx) -> String {
        self.shell(cx).borrow::<shell::Shell>().map(|s| {
            format!("{}\n\n{}", s.weather_summary(), s.schedule_summary())
        }).unwrap_or_default()
    }

    fn ai_answer(&self, cx: &mut Cx, call: &makepad_ai_services::wire::ServiceCall) -> makepad_ai_services::wire::ToolResult {
        self.shell(cx)
            .borrow::<shell::Shell>()
            .map(|s| ai::answer(&s, call))
            .unwrap_or_else(|| makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "the weather agent's screen is gone"))
    }

    fn refresh_ai_context(&mut self, cx: &mut Cx) {
        if self.ai_port.is_none() {
            return;
        }
        let text = self.ai_summary(cx);
        if text == self.ai_context {
            return;
        }
        self.ai_context = text.clone();
        if let Some(port) = self.ai_port.as_ref() {
            port.set_context(&text);
        }
    }

    fn drain_ai_port(&mut self, cx: &mut Cx, event: &Event) {
        let events = match self.ai_port.as_mut() {
            Some(port) => port.handle_event(cx, event),
            None => return,
        };
        for ev in events {
            match ev {
                PortEvent::Registered(endpoint) => {
                    log!("weather-agent: AI service registered as {}", endpoint.as_str());
                    self.ai_context.clear();
                    self.refresh_ai_context(cx);
                }
                PortEvent::Call(call) => {
                    let result = self.ai_answer(cx, &call);
                    if let Some(port) = self.ai_port.as_ref() {
                        port.reply(result);
                    }
                }
                // 两个工具都是就地答掉的读操作，没有能取消的长任务，也没有
                // 自己的会话要挪开。
                PortEvent::Cancel { .. } | PortEvent::ChatOpen { .. } => {}
                PortEvent::Subscribe { .. } | PortEvent::Unsubscribe { .. } => {}
            }
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // 兜底：shell 那边 broker 注入经常掉链子（policy / 启动时序），所以
        // 拿不到 broker 的时候，自己起内核 + 注 broker 顶上。这样独立窗格
        // 和 shell 托管都能用 AI。
        let scope = format!("standalone-{}", std::process::id());
        let assistant = octosense_weather_agent::standalone::bootstrap_if_needed(&scope);
        if let Some(mut s) = self.shell(cx).borrow_mut::<shell::Shell>() {
            s.attach_assistant(assistant, scope);
            s.start_fetch(cx);
            s.render_all(cx);
        }
        self.ai_port = AiServicePort::open(cx, ai::manifest());
        makepad_wm_api::set_title(cx, "天气助手");
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        makepad_wm_theme::apply(vm);
        // 助手的面板和覆盖层根，窗的 F10 槽靠名字找 `mod.widgets.AiChatOverlay`。
        makepad_aichat::script_mod(vm);
        octosense_weather_agent::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // 窗口管理器礼貌地请求关闭（SUPER+W）：那就走。
        if let Event::Custom(json) = event {
            if let Some(makepad_wm_api::WmEvent::CloseRequested) = makepad_wm_api::WmEvent::parse(json) {
                cx.quit();
                return;
            }
        }
        self.drain_ai_port(cx, event);
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        self.refresh_ai_context(cx);
    }
}

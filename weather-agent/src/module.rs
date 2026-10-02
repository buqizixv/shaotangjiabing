// weather-agent as a module: the app a host seats in a tile in-process, in an
// isolate of its own, next to `reference`, `sheets` and `weather`.
//
// `register` puts this crate's widget family and the root type into the isolate
// the host prepared; `create` mints one `Shell{}` root there, starts the first
// forecast, and hands the host the app's tools — the same two the standalone
// binary serves over its port, answered on the root at call time. The module
// never opens a socket or starts a model of its own: the forecast fetch rides
// the platform's own HTTP request API, and the assistant is the shell's.

use crate::shell::Shell;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult};
use makepad_app_module::*;
use makepad_widgets::*;

pub struct WeatherAgentModule;

/// The one linked instance of the module description: immutable, no state.
pub static WEATHER_AGENT_MODULE: WeatherAgentModule = WeatherAgentModule;

impl AppModule for WeatherAgentModule {
    fn id(&self) -> &'static str {
        "weather-agent"
    }

    fn label(&self) -> &'static str {
        "天气助手"
    }

    fn register(&self, vm: &mut ScriptVm) {
        crate::script_mod(vm);
    }

    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }

    /// The forecast and the schedule the assistant may read, plus the
    /// assistant services themselves: host policy decides whether the app gets
    /// the shell's peer (it does not see the profile, the credentials or a raw
    /// kernel). The grant is keyed by THIS id, `weather-agent`; the AI bus's
    /// service id is `weather_agent` (src/ai.rs), because the bus forbids
    /// hyphens in service ids.
    fn capabilities(&self) -> &'static [&'static str] {
        &[
            "net",
            "octos.session.open",
            "octos.session.history",
            "octos.turn.start",
            "octos.turn.interrupt",
        ]
    }

    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        let scope = handles.scope.to_string();
        let assistant = octosense_app_peers::injection::claim(self.id(), &scope);
        let value = script_eval!(vm, {
            use mod.widgets.*
            Shell {}
        });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut shell) = root.borrow_mut::<Shell>() {
            shell.attach_assistant(assistant, scope);
            shell.set_storage(vm.cx_mut(), handles.storage);
            // `with_cx_mut` parks the VM back onto `Cx` for the closure, so the
            // `script_apply_eval!` inside `render_all` can take it again.
            vm.with_cx_mut(|cx| {
                shell.start_fetch(cx);
                shell.render_all(cx);
            });
        }
        let shutdown_root = root.clone();
        InstanceParts {
            root: root.clone(),
            executor: Box::new(WeatherAgentExecutor { root }),
            shutdown: Box::new(move |vm| {
                if let Some(mut shell) = shutdown_root.borrow_mut::<Shell>() {
                    vm.with_cx_mut(|cx| shell.shutdown(cx));
                }
            }),
        }
    }
}

/// The instance's two tools, read from the root at call time.
struct WeatherAgentExecutor {
    root: WidgetRef,
}

impl ServiceExecutor for WeatherAgentExecutor {
    fn manifest(&self) -> ServiceManifest {
        crate::ai::manifest()
    }

    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        let result = self
            .root
            .borrow::<Shell>()
            .map(|shell| crate::ai::answer(&shell, call))
            .unwrap_or_else(|| ToolResult::unavailable(&call.call_id, "the weather agent's screen is gone"));
        ExecOutcome::Done(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_ai_services::wire::ToolOutcome;
    use makepad_widgets::widget_async::{enter_isolate, leave_isolate};

    #[test]
    fn the_module_describes_itself_and_opens_empty() {
        let m = &WEATHER_AGENT_MODULE;
        assert_eq!(m.id(), "weather-agent");
        assert!(m.capabilities().contains(&"net"));
        for service in ["octos.session.open", "octos.session.history", "octos.turn.start", "octos.turn.interrupt"] {
            assert!(m.capabilities().contains(&service), "the app declares the assistant services it needs");
        }
        let schema = m.open_schema();
        assert_eq!(schema.version, 1);
        assert!(schema.empty_open().is_ok(), "no argument is required");
    }

    /// The whole contract without a window manager: an instance in an isolate
    /// of its own, all four tabs still switching and the tools still answering
    /// from the root.
    #[test]
    fn the_module_mints_its_root_in_a_fresh_isolate_and_the_tools_answer() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(makepad_widgets::script_mod);
        let vm_id = cx.alloc_splash_vm_with_network(false);
        let (replies, _upstream) = ReplySink::pair();
        let handles = InstanceHandles {
            scope: InstanceScope::new(1, 1),
            storage: cx.storage("weather-agent.test"),
            viewport: Viewport { size: dvec2(430.0, 860.0) },
            replies,
            windows: Default::default(),
        };
        let open = WEATHER_AGENT_MODULE.open_schema().empty_open().unwrap();
        let InstanceParts { root, mut executor, shutdown } = cx.with_script_vm_id_trusted(vm_id, |vm| {
            WEATHER_AGENT_MODULE.register(vm);
            let parts = WEATHER_AGENT_MODULE.create(vm, open, handles);
            assert!(vm.take_errors().is_empty(), "the isolate evaluated the shell without errors");
            parts
        });
        assert!(root.borrow::<Shell>().is_some(), "the root is a Shell");

        let entry = enter_isolate(&mut cx, vm_id);
        let shell = root.borrow::<Shell>().unwrap();
        let summary = shell.weather_summary();
        assert!(summary.contains("北京"), "the default city is named: {summary}");
        assert!(shell.schedule_summary().contains("日程"), "the schedule tool reads the day's items");
        drop(shell);
        leave_isolate(&mut cx, entry);

        // Both tools answer from the root, and a name that does not exist is
        // refused with the names that do.
        fn take(call: ServiceCall, executor: &mut dyn ServiceExecutor, cx: &mut Cx) -> ToolResult {
            match executor.execute(cx, &call) {
                ExecOutcome::Done(result) => result,
                // Both tools read the root and answer on the spot.
                ExecOutcome::Pending => panic!("the tool said the work continues"),
            }
        }
        let call = |id: &str, tool: &str| ServiceCall {
            call_id: id.to_string(),
            tool: tool.to_string(),
            args: r#"{"type":"object","properties":{}}"#.to_string(),
        };

        let current = take(call("c1", "current"), executor.as_mut(), &mut cx);
        assert_eq!(current.outcome, ToolOutcome::Ok);
        assert!(current.text.contains("北京"), "the forecast is read from the root: {}", current.text);

        let schedule = take(call("c2", "schedule"), executor.as_mut(), &mut cx);
        assert_eq!(schedule.outcome, ToolOutcome::Ok);
        assert!(schedule.text.contains("日程"), "the schedule is read from the root: {}", schedule.text);

        let refused = take(call("c3", "past"), executor.as_mut(), &mut cx);
        assert_eq!(refused.outcome, ToolOutcome::Failed, "an unknown tool is refused");
        assert!(refused.text.contains("past"));

        // The host's order: shutdown in the isolate, the refs, the isolate.
        cx.with_script_vm_id_trusted(vm_id, |vm| shutdown(vm));
        drop(root);
        drop(executor);
        cx.free_splash_vm(vm_id);
    }
}

// weather-agent on the AI bus: two bounded read tools over the data on screen —
// the forecast and the schedule. The shell's assistant (the model the system is
// configured with) calls them and answers from the result, so the app itself
// never holds a key and never starts a model.
//
// Tool names are SHORT: the bus forms the canonical name itself, so the model
// sees `weather_agent.current` and `weather_agent.schedule` — `[a-z0-9_]` only,
// no dots (native function-calling APIs forbid them) and no hyphens (the bus's
// own identifier rule), so the service id is `weather_agent` while the app's
// launcher and module id stay `weather-agent`.
//
// The same manifest is served two ways: standalone through the window's own port
// (src/main.rs), hosted through the module executor (src/module.rs).

use crate::shell::Shell;
use makepad_ai_services::wire::{Risk, ServiceCall, ServiceManifest, ToolDef, ToolResult};

const NO_ARGS: &str = r#"{"type":"object","properties":{}}"#;

/// The manifest shared by the standalone port and the module executor.
pub fn manifest() -> ServiceManifest {
    ServiceManifest::new(
        "weather_agent",
        "天气助手",
        "The weather agent's own screen: the Open-Meteo forecast for the selected city and the day schedule on display. Read-only.",
    )
    .with_tool(ToolDef::new(
        "current",
        "Current conditions and the forecast for the city selected on screen: now (temperature, feels like, humidity, precipitation, cloud, wind), the next 24 hours (high, low, the hours whose rain chance is 50% or more), the next five days, and sunrise and sunset.",
        NO_ARGS,
        Risk::Read,
    ))
    .with_tool(ToolDef::new(
        "schedule",
        "The items the day schedule page is showing right now, with their date, time, place, and whether each is done.",
        NO_ARGS,
        Risk::Read,
    ))
}

/// Answer one call against the screen. Every branch answers; an unknown name is
/// refused with the names that exist.
pub fn answer(shell: &Shell, call: &ServiceCall) -> ToolResult {
    match call.tool.as_str() {
        "current" => ToolResult::ok(&call.call_id, shell.weather_summary(), ""),
        "schedule" => ToolResult::ok(&call.call_id, shell.schedule_summary(), ""),
        other => ToolResult::failed(
            &call.call_id,
            format!("unknown tool `{other}`; this app has `current` and `schedule`"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_declares_two_read_tools() {
        let m = manifest();
        assert_eq!(m.id, "weather_agent");
        assert!(m.validate().is_ok(), "ids and tool names are bus-legal");
        assert_eq!(m.tools.len(), 2);
        assert_eq!(m.tools[0].name, "current");
        assert_eq!(m.tools[1].name, "schedule");
        assert_eq!(m.tools[0].risk, Risk::Read);
        assert_eq!(m.tools[1].risk, Risk::Read);
    }

    #[test]
    fn the_tools_take_no_arguments() {
        for tool in manifest().tools {
            assert_eq!(tool.parameters, NO_ARGS);
        }
    }
}

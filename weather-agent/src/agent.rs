//! Weather chat through the OctoSense host's scoped system assistant.
//!
//! Hosted weather-agent instances claim an `OctosAppService` injected by the
//! shell. The model, provider and credentials stay in the host; this app only
//! sends a weather-grounded prompt and receives the assistant's response.

use std::sync::{mpsc, Arc};

use makepad_widgets::makepad_platform::thread::SignalToUI;
use octosense_app_peers::{
    Availability, ContextEvent, ContextOp, ContextSpec, EventSink, OctosAppService, OctosContext,
};
use serde_json::Value;

use crate::model::{self, City, Forecast};

/// The account every weather-agent instance binds to: the app has no
/// identity of its own, so the host scopes the peer's memory to the instance.
pub const ACCOUNT: &str = "local";

/// The badge text for a granted service, judged by the broker's OWN
/// availability rather than by a model string. The kernel's `peer/prepare`
/// currently answers with just `{"lane": "primary"}`, so `model()` never
/// carries provider or model even when the assistant is perfectly healthy —
/// reading "no model name" as "not configured" is what kept the badge
/// stuck on「AI 助手未配置」for good.
pub fn assistant_label(service: &dyn OctosAppService) -> String {
    match service.availability() {
        Availability::Ready => match service.model() {
            Some(model) if model.model.is_some() || model.provider.is_some() => {
                model.model.or(model.provider).unwrap_or_else(|| "已配置模型".into())
            }
            _ => "AI 助手已连接".into(),
        },
        Availability::Idle => "AI 助手连接中…".into(),
        Availability::Failed(_) => "AI 助手连接失败".into(),
        Availability::Unavailable(_) => "AI 助手未连接".into(),
    }
}

#[derive(Debug)]
pub enum AgentUpdate {
    Progress(String),
    Complete(String),
    Failed(String),
}

pub struct AgentClient {
    context: Arc<dyn OctosContext>,
    tx: mpsc::Sender<AgentUpdate>,
    rx: mpsc::Receiver<AgentUpdate>,
    busy: bool,
    pub model_label: String,
}

impl AgentClient {
    pub fn new(service: Arc<dyn OctosAppService>, scope: &str) -> Result<Self, String> {
        let services = service.services();
        if !services.contains("octos.turn.start") || !services.contains("octos.session.open") {
            return Err("系统助手未授予天气应用对话权限".into());
        }
        // This app has no separate account identity; the host binds the
        // context to its local account and scopes its memory to this instance.
        let account = ACCOUNT.to_string();
        service.set_account(Some(ACCOUNT));
        let context = service.open_context(ContextSpec {
            account,
            instance: scope.to_string(),
            services,
        })?;
        // Badge text judged by availability, see `assistant_label`.
        let model_label = assistant_label(&*service);
        let (tx, rx) = mpsc::channel();
        Ok(Self { context, tx, rx, busy: false, model_label })
    }

    pub fn ask(&mut self, text: &str, city: &City, forecast: &Option<Forecast>, status_ok: bool, profile: &str) -> Result<(), String> {
        if self.busy {
            return Err("上一条消息还在处理中".into());
        }
        let prompt = format!(
            "你是 OctoSense 天气助手。请用中文简洁回答，先给结论；不确定的信息要说明。\n\
             只输出一个 JSON 对象，不要 Markdown 代码围栏，字段为 title（卡片标题）、body（回答正文）、choices（至多 3 个可点击的后续选项数组）。\n\
             只有当用户明确要求新建日程时，才在 schedule 字段给出对象 {{\"title\":\"事项\",\"date\":\"YYYY-MM-DD\",\"time\":\"HH:MM\",\"place\":\"地点或空串\"}}；此时 body 说明将要添加，并把 choices 设为 [\"同意\",\"拒绝\"]。其他情况 schedule 必须为 null。\n\
             今天是 {}。当前所选位置：{}（{}）。\n\
             用户主动填写的个性化资料（只在相关时使用，不要推断未填写的信息）：{}\n\
             以下是天气应用刚刚取得的真实天气摘要，请只依据其中数据回答天气问题：\n{}\n\
             用户问题：{}",
            model::today_local(),
            city.name,
            city.en,
            profile,
            model::ai_summary(city, forecast, status_ok),
            text.trim(),
        );
        let tx = self.tx.clone();
        let sink: EventSink = Arc::new(move |event| {
            let update = match event {
                ContextEvent::Data(data) => data.get("text").and_then(Value::as_str)
                    .map(|text| AgentUpdate::Progress(text.to_string())),
                ContextEvent::Complete(Ok(value)) => {
                    let text = value.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
                    if text.trim().is_empty() {
                        Some(AgentUpdate::Failed("天气助手返回了空回复，请再试一次。".into()))
                    } else {
                        Some(AgentUpdate::Complete(text))
                    }
                }
                ContextEvent::Complete(Err(error)) => Some(AgentUpdate::Failed(error)),
            };
            if let Some(update) = update {
                let _ = tx.send(update);
                SignalToUI::set_ui_signal();
            }
        });
        self.context.call(ContextOp::Turn { text: prompt }, sink)?;
        self.busy = true;
        Ok(())
    }

    pub fn cancel(&mut self) {
        let sink: EventSink = Arc::new(|_| {});
        let _ = self.context.call(ContextOp::Interrupt, sink);
    }

    pub fn busy(&self) -> bool { self.busy }

    pub fn drain(&mut self) -> Vec<AgentUpdate> {
        let mut updates = Vec::new();
        while let Ok(update) = self.rx.try_recv() {
            if matches!(update, AgentUpdate::Complete(_) | AgentUpdate::Failed(_)) {
                self.busy = false;
            }
            updates.push(update);
        }
        updates
    }
}

impl Drop for AgentClient {
    fn drop(&mut self) {
        self.context.close();
    }
}

//! Persisted, reconnectable public run events.
use crate::{AssistantOutput, TaskStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RUN_EVENT_SCHEMA_V1: &str = "aethra.run-event.v1";
pub const RUN_EVENT_PAGE_SCHEMA_V1: &str = "aethra.run-events.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunEventKind {
    RunStarted,
    TaskState,
    WorkerStarted,
    TextDelta,
    WorkerTerminal,
    OutputUpsert,
    RunPaused,
    RunTerminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunEventEnvelope {
    pub schema: String,
    pub event_id: String,
    pub sequence: u64,
    pub run_id: Uuid,
    /// Compatibility identifier for clients using the original task event shape.
    pub task_id: Uuid,
    pub worker_id: Option<Uuid>,
    pub kind: RunEventKind,
    pub status: TaskStatus,
    pub step: u32,
    pub message: String,
    #[serde(default)]
    pub text_delta: Option<String>,
    pub output: Option<AssistantOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRunEvent {
    pub run_id: Uuid,
    pub worker_id: Option<Uuid>,
    pub kind: RunEventKind,
    pub status: TaskStatus,
    pub step: u32,
    pub message: String,
    pub text_delta: Option<String>,
    pub output: Option<AssistantOutput>,
}

impl NewRunEvent {
    pub fn validate(&self) -> crate::Result<()> {
        if self.message.len() > 4_000 {
            return Err(crate::Error::InvalidInput);
        }
        match (&self.kind, &self.text_delta) {
            (RunEventKind::TextDelta, Some(delta))
                if !delta.is_empty() && delta.len() <= 16_000 => {}
            (RunEventKind::TextDelta, _) => return Err(crate::Error::InvalidInput),
            (_, Some(_)) => return Err(crate::Error::InvalidInput),
            _ => {}
        }
        if matches!(
            self.kind,
            RunEventKind::WorkerStarted | RunEventKind::TextDelta | RunEventKind::WorkerTerminal
        ) && self.worker_id.is_none()
        {
            return Err(crate::Error::InvalidInput);
        }
        match (&self.kind, &self.output) {
            (RunEventKind::OutputUpsert, Some(output)) => output.validate()?,
            (RunEventKind::OutputUpsert, None) | (_, Some(_)) => {
                return Err(crate::Error::InvalidInput)
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunEventPage {
    pub schema: String,
    pub after: u64,
    pub next_after: u64,
    pub has_more: bool,
    pub reset_required: bool,
    pub events: Vec<RunEventEnvelope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventBounds {
    pub oldest: u64,
    pub newest: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: RunEventKind, text_delta: Option<&str>) -> NewRunEvent {
        NewRunEvent {
            run_id: Uuid::new_v4(),
            worker_id: Some(Uuid::new_v4()),
            kind,
            status: TaskStatus::Running,
            step: 1,
            message: String::new(),
            text_delta: text_delta.map(str::to_owned),
            output: None,
        }
    }

    #[test]
    fn only_bounded_text_delta_events_accept_delta_content() {
        assert_eq!(
            event(RunEventKind::TextDelta, Some("token")).validate(),
            Ok(())
        );
        assert_eq!(
            event(RunEventKind::TextDelta, Some("")).validate(),
            Err(crate::Error::InvalidInput)
        );
        assert_eq!(
            event(RunEventKind::WorkerStarted, Some("token")).validate(),
            Err(crate::Error::InvalidInput)
        );
        assert_eq!(
            event(RunEventKind::OutputUpsert, None).validate(),
            Err(crate::Error::InvalidInput)
        );
    }
}

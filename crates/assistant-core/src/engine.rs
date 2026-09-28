use assistant_contracts::*;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::Duration,
};
use tokio::sync::Mutex;

pub struct Assistant {
    pub store: Arc<dyn Store>,
    fast: Arc<dyn ModelProvider>,
    pub(crate) cloud: Arc<dyn ModelProvider>,
    executor: Arc<dyn ToolExecutor>,
    pub(crate) config: EngineConfig,
    // ponytail: one active task runner; use per-task leases for concurrent execution.
    runner: Mutex<()>,
    pub(crate) personal_guard: super::personalization::PersonalDataGuard,
    pub(crate) learning: Mutex<()>,
}

impl Assistant {
    pub fn new(
        store: Arc<dyn Store>,
        fast: Arc<dyn ModelProvider>,
        cloud: Arc<dyn ModelProvider>,
        executor: Arc<dyn ToolExecutor>,
        config: EngineConfig,
    ) -> Self {
        Self {
            store,
            fast,
            cloud,
            executor,
            config,
            runner: Mutex::new(()),
            personal_guard: Arc::new(|_| Ok(())),
            learning: Mutex::new(()),
        }
    }
    pub fn submit(&self, input: UserInput) -> Result<Task> {
        if input.text.trim().is_empty() || input.text.len() > 8000 {
            return Err(Error::InvalidInput);
        }
        let task = Task::new(input);
        self.store.save_task(&task)?;
        Ok(task)
    }
    pub fn with_personal_guard(mut self, guard: super::personalization::PersonalDataGuard) -> Self {
        self.personal_guard = guard;
        self
    }
    pub fn cancel(&self, id: Id) -> Result<Task> {
        let mut task = self.store.task(id)?;
        if !task.status.terminal() {
            if let Some(graph) = task.work_graph.as_mut() {
                graph.cancel_remaining();
            }
            task.status = TaskStatus::Cancelled;
            task.message = if task.pending.as_ref().is_some_and(|p| p.started) {
                "Cancelled. An action already in flight may still finish."
            } else {
                "Cancelled."
            }
            .into();
            self.store.save_task(&task)?;
        }
        Ok(task)
    }
    pub fn approve(&self, id: Id, approval_id: Id, approved: bool) -> Result<Task> {
        let mut task = self.store.task(id)?;
        if task.status != TaskStatus::WaitingForApproval {
            return Err(Error::Conflict);
        }
        let pending = task.pending.as_mut().ok_or(Error::Conflict)?;
        if pending.id != approval_id || pending.started {
            return Err(Error::Conflict);
        }
        let Capability::Tool(current) = self.store.capability(&pending.call.tool_id)? else {
            return Err(Error::Denied);
        };
        if current != pending.spec || !current.enabled {
            return Err(Error::Conflict);
        }
        pending.approved = approved;
        task.status = if approved {
            TaskStatus::Created
        } else {
            TaskStatus::Cancelled
        };
        self.store.save_task(&task)?;
        Ok(task)
    }
    pub fn answer(&self, id: Id, answer: &str) -> Result<Task> {
        let mut task = self.store.task(id)?;
        if task.status != TaskStatus::WaitingForUser
            || answer.trim().is_empty()
            || answer.len() > 4000
        {
            return Err(Error::Conflict);
        }
        let result_id = Id::new_v4();
        self.store
            .save_result(result_id, &serde_json::json!({"user_answer":answer}))?;
        task.result_refs.push(result_id);
        task.status = TaskStatus::Created;
        self.store.save_task(&task)?;
        Ok(task)
    }
    /// Retry after credentials are repaired; the provider or tool adapter rechecks authentication.
    pub fn resume_auth(&self, id: Id) -> Result<Task> {
        let mut task = self.store.task(id)?;
        if task.status != TaskStatus::WaitingForAuth {
            return Err(Error::Conflict);
        }
        task.status = TaskStatus::Created;
        self.store.save_task(&task)?;
        self.store.delete_task_blocker(id)?;
        Ok(task)
    }

    /// The typed blocker for a paused task, if one was recorded.
    /// Legacy rows without a record behave exactly as before.
    pub fn current_blocker(&self, id: Id) -> Result<Option<TaskBlocker>> {
        let task = self.store.task(id)?;
        if task.status != TaskStatus::WaitingForAuth {
            return Ok(None);
        }
        self.store.task_blocker(id)
    }
    /// Requeues safe graph work left by a previous process and returns tasks ready to run.
    pub fn recover_unfinished_graphs(&self) -> Result<Vec<Id>> {
        let mut ready = Vec::new();
        for mut task in self.store.unfinished_tasks()? {
            if !matches!(task.status, TaskStatus::Created | TaskStatus::Running)
                || task.work_graph.is_none()
            {
                continue;
            }
            if task.pending.is_some() {
                if task.pending.as_ref().is_some_and(|pending| pending.started) {
                    task.status = TaskStatus::WaitingForResolution;
                    task.message =
                        "An interrupted action needs its outcome checked before continuing.".into();
                    self.store.save_task(&task)?;
                }
                continue;
            }
            match super::graph::recover(&mut task, self.store.as_ref()) {
                Ok(should_run) => {
                    self.store.save_task(&task)?;
                    if should_run {
                        ready.push(task.id);
                    }
                }
                Err(_) => {
                    task.status = TaskStatus::WaitingForResolution;
                    task.message = "Saved parallel work could not be safely validated.".into();
                    self.store.save_task(&task)?;
                }
            }
        }
        Ok(ready)
    }

    pub fn recover_unfinished_tasks(&self) -> Result<Vec<Id>> {
        let mut ready = Vec::new();
        for mut task in self.store.unfinished_tasks()? {
            if task.work_graph.is_some()
                || !matches!(task.status, TaskStatus::Created | TaskStatus::Running)
            {
                continue;
            }
            if task.status == TaskStatus::Created && task.pending.is_none() {
                ready.push(task.id);
                continue;
            }
            match task.pending.as_ref() {
                Some(pending) if pending.started => {
                    task.status = TaskStatus::WaitingForResolution;
                    task.message =
                        "An interrupted action needs its outcome checked before continuing.".into();
                    self.store.save_task(&task)?;
                }
                Some(pending) if !pending.approved => {
                    task.status = TaskStatus::WaitingForApproval;
                    task.message = "Review the pending action before it runs.".into();
                    self.store.save_task(&task)?;
                }
                _ => {
                    task.status = TaskStatus::Created;
                    task.message = "Task resumed after an interrupted run.".into();
                    self.store.save_task(&task)?;
                    ready.push(task.id);
                }
            }
        }
        Ok(ready)
    }
    pub async fn run(&self, id: Id) -> Result<Task> {
        let _guard = self.runner.lock().await;
        let mut task = self.store.task(id)?;
        if task.status.terminal() {
            return Ok(task);
        }
        if !matches!(task.status, TaskStatus::Created | TaskStatus::Running) {
            return Ok(task);
        }
        if task.pending.as_ref().is_some_and(|p| p.started) {
            task.status = TaskStatus::WaitingForResolution;
            task.message =
                "An interrupted action needs its outcome checked before continuing.".into();
            self.store.save_task(&task)?;
            return Ok(task);
        }
        task.status = TaskStatus::Running;
        self.store.save_task(&task)?;
        loop {
            let latest = self.store.task(id)?;
            if latest.status == TaskStatus::Cancelled {
                return Ok(latest);
            }
            if task.status != TaskStatus::Running {
                return Ok(task);
            }
            if task
                .work_graph
                .as_ref()
                .is_some_and(|graph| graph.nodes.iter().any(|node| !node.state.terminal()))
            {
                super::graph::run(
                    &mut task,
                    self.store.clone(),
                    self.cloud.clone(),
                    self.executor.clone(),
                    &self.config,
                )
                .await?;
                continue;
            }
            if task.step >= self.config.max_steps || task.failures >= self.config.max_failures {
                task.status = TaskStatus::Failed;
                task.message = if task.failures >= self.config.max_failures {
                    format!("Task stopped after repeated errors: {}", task.message)
                } else {
                    "Task execution limit reached.".into()
                };
                self.store.save_task(&task)?;
                return Ok(task);
            }
            if task.pending.is_some() {
                self.execute_pending(&mut task).await?;
                continue;
            }
            let context = match super::context::build(self.store.as_ref(), &task, &self.config) {
                Ok(context) => context,
                Err(error) => {
                    self.failure(&mut task, error)?;
                    continue;
                }
            };
            // A tool-only provider cannot answer when retrieval found no executable tools.
            if task.role == Role::Fast
                && context.tools.is_empty()
                && !self.fast.capabilities().planning
            {
                task.role = Role::Reasoner;
                self.store.save_task(&task)?;
                continue;
            }
            task.step += 1;
            self.store.record_personal_usage(
                task.id,
                task.input.conversation_id,
                &context.adaptive_rules,
            )?;
            for skill in &context.skills {
                if let Some(id) = skill
                    .id
                    .strip_prefix("personal:")
                    .and_then(|id| Id::parse_str(id).ok())
                {
                    let rule = self.store.adaptive_rule(id)?;
                    self.store.record_personal_usage(
                        task.id,
                        task.input.conversation_id,
                        &[rule],
                    )?;
                }
            }
            let exposed = context.tools.clone();
            let provider = if task.role == Role::Fast {
                &self.fast
            } else {
                &self.cloud
            };
            let worker_id = Id::new_v4();
            self.store.append_event(&NewRunEvent {
                run_id: task.id,
                worker_id: Some(worker_id),
                kind: RunEventKind::WorkerStarted,
                status: TaskStatus::Running,
                step: task.step,
                message: provider.id().to_owned(),
                text_delta: None,
                output: None,
            })?;
            let event_store = self.store.clone();
            let run_id = task.id;
            let step = task.step;
            let worker_active = Arc::new(AtomicBool::new(true));
            let sink_active = worker_active.clone();
            let streamed_bytes = Arc::new(AtomicUsize::new(0));
            let sink_bytes = streamed_bytes.clone();
            let answered_by: Arc<StdMutex<Option<String>>> = Arc::new(StdMutex::new(None));
            let answered_by_slot = answered_by.clone();
            let sink: ProviderEventSink = Arc::new(move |event| {
                if !sink_active.load(Ordering::Acquire)
                    || event_store.task(run_id)?.status == TaskStatus::Cancelled
                {
                    return Ok(());
                }
                match event {
                    ProviderEvent::ProviderSelected { provider_id } => {
                        // Record which vendor is answering this turn. A fallback
                        // that stays invisible would misattribute the answer.
                        *answered_by_slot.lock().map_err(|_| Error::Conflict)? = Some(provider_id);
                        Ok(())
                    }
                    ProviderEvent::TextDelta { text } => {
                        if sink_bytes
                            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                                current
                                    .checked_add(text.len())
                                    .filter(|total| *total <= 256_000)
                            })
                            .is_err()
                        {
                            return Err(Error::InvalidResponse);
                        }
                        event_store.append_event(&NewRunEvent {
                            run_id,
                            worker_id: Some(worker_id),
                            kind: RunEventKind::TextDelta,
                            status: TaskStatus::Running,
                            step,
                            message: String::new(),
                            text_delta: Some(text),
                            output: None,
                        })
                    }
                }
            });
            let outcome = tokio::time::timeout(
                Duration::from_secs(self.config.timeout_seconds),
                provider.infer_stream(context, sink),
            )
            .await
            .unwrap_or(Err(Error::Timeout));
            worker_active.store(false, Ordering::Release);
            let latest = self.store.task(id)?;
            if latest.status == TaskStatus::Cancelled {
                return Ok(latest);
            }
            self.store.append_event(&NewRunEvent {
                run_id: task.id,
                worker_id: Some(worker_id),
                kind: RunEventKind::WorkerTerminal,
                status: TaskStatus::Running,
                step: task.step,
                message: if outcome.is_ok() {
                    "Provider step completed."
                } else {
                    "Provider step failed."
                }
                .into(),
                text_delta: None,
                output: None,
            })?;
            match outcome {
                Ok(action) => {
                    // Attribute the turn to the provider that actually answered.
                    if let Ok(slot) = answered_by.lock() {
                        if let Some(provider_id) = slot.as_ref() {
                            task.answered_by = Some(provider_id.clone());
                        }
                    }
                    if let Err(error) = self.apply(&mut task, action, &exposed) {
                        self.failure(&mut task, error)?;
                    } else {
                        self.store.save_task(&task)?;
                    }
                }
                Err(Error::AuthRequired) if task.role != Role::Fast => {
                    task.status = TaskStatus::WaitingForAuth;
                    task.message = "Cloud authentication is required. Update the API key in Settings, then resume.".into();
                    self.store.save_task(&task)?;
                    self.store.save_task_blocker(
                        task.id,
                        &TaskBlocker::ProviderCredentialRequired {
                            provider_id: self.cloud.id().to_owned(),
                        },
                    )?;
                }
                Err(Error::RateLimited) if task.role != Role::Fast => {
                    task.failures += 1;
                    task.status = TaskStatus::Failed;
                    task.message = "The cloud provider's rate limit or quota was reached. Try a new task after its retry window, or continue in local chat.".into();
                    self.store.save_task(&task)?;
                    return Ok(task);
                }
                Err(error) => self.failure(&mut task, error)?,
            }
        }
    }
    /// A tool that could not run is not a cloud-model problem. Reporting it as
    /// one sends the user to the provider screen when the real fix is the
    /// connection this tool needs, so name the tool and the cause.
    fn tool_failure(&self, task: &mut Task, tool: &ToolSpec, error: Error) -> Result<()> {
        task.failures += 1;
        task.status = TaskStatus::Failed;
        task.message = match error {
            Error::Unavailable | Error::AuthRequired => {
                format!(
                    "{} could not run because its connection is not available. Connect or enable {} in Settings, then try again.",
                    tool.name, tool.connection_id
                )
            }
            Error::Denied => format!(
                "{} was blocked before it ran. Check the tool's permissions in Settings.",
                tool.name
            ),
            Error::InvalidInput => format!(
                "{} was called with arguments it cannot accept, so it did not run.",
                tool.name
            ),
            other => format!("{} failed: {}", tool.name, other),
        };
        self.store.save_task(task)
    }

    fn failure(&self, task: &mut Task, error: Error) -> Result<()> {
        task.failures += 1;
        task.message = error.to_string();
        if error == Error::Unavailable && task.role != Role::Fast {
            task.status = TaskStatus::Failed;
            // An unreachable provider is not a settings problem, and telling the
            // user to check their settings sends them somewhere that cannot fix a
            // dropped connection. A credential or model fault arrives as a
            // different error and keeps its own accurate message.
            task.message = "No reasoning model is reachable right now, so this could not be answered. Reconnect or configure a provider and try again.".into();
            self.store.save_task(task)?;
            return self
                .store
                .save_task_blocker(
                    task.id,
                    &TaskBlocker::ReasoningUnavailable {
                        detail: error.to_string(),
                    },
                )
                .map(|_| ());
        }
        if task.role == Role::Fast {
            task.role = Role::Reasoner;
            task.handoff = Some(Handoff {
                task_id: task.id,
                plan_revision: task.plan_revision,
                objective: task.input.text.clone(),
                reason: error.to_string(),
                completed_steps: task.step,
                result_refs: task.result_refs.clone(),
            });
        }
        self.store.save_task(task)
    }
    fn apply(&self, task: &mut Task, action: AgentAction, exposed: &[ToolSpec]) -> Result<()> {
        match action {
            AgentAction::CallTool { call } => {
                let selected = exposed
                    .iter()
                    .find(|t| t.id == call.tool_id && t.version == call.version)
                    .ok_or(Error::Denied)?;
                let Capability::Tool(current) = self.store.capability(&call.tool_id)? else {
                    return Err(Error::Denied);
                };
                if &current != selected {
                    return Err(Error::Conflict);
                }
                super::registry::validate_call(&current, &call)?;
                let fingerprint = serde_json::to_string(&call).map_err(|_| Error::InvalidInput)?;
                let count = task.call_counts.entry(fingerprint).or_default();
                *count += 1;
                if *count > self.config.max_identical_calls {
                    return Err(Error::Denied);
                }
                let needs_approval = current.risk.requires_approval();
                // Record the choice on the task before the pending action can be
                // cleared, so what a turn decided is still knowable afterwards.
                task.invoked_tool = Some(selected.id.clone());
                task.pending = Some(PendingAction {
                    id: Id::new_v4(),
                    call,
                    spec: current,
                    approved: false,
                    started: false,
                });
                if needs_approval {
                    task.status = TaskStatus::WaitingForApproval;
                    task.message = "Review this action before it runs.".into();
                }
            }
            AgentAction::Search { query } => {
                if query.trim().is_empty() || query.len() > 2000 {
                    return Err(Error::InvalidInput);
                }
                if query.trim().eq_ignore_ascii_case(task.search_query.trim()) {
                    return Err(Error::InvalidResponse);
                }
                task.search_query = query;
            }
            AgentAction::ActivateSkill { skill_id } => {
                if let Some(id) = skill_id
                    .strip_prefix("personal:")
                    .and_then(|id| Id::parse_str(id).ok())
                {
                    if !self
                        .store
                        .personal_rules(
                            task.input.conversation_id,
                            None,
                            super::personalization::now(),
                        )?
                        .iter()
                        .any(|rule| rule.id == id)
                    {
                        return Err(Error::Denied);
                    }
                }
                let skill = super::registry::activate(self.store.as_ref(), &skill_id)?;
                task.active_skills.clear();
                task.active_skills.push(skill);
            }
            AgentAction::Plan { steps } => {
                if steps.is_empty()
                    || steps.len() > 12
                    || steps.iter().any(|s| s.is_empty() || s.len() > 500)
                {
                    return Err(Error::InvalidInput);
                }
                task.plan = steps;
                task.plan_revision += 1;
                task.role = Role::Fast;
            }
            AgentAction::PlanGraph { proposal } => {
                if task
                    .work_graph
                    .as_ref()
                    .is_some_and(|graph| graph.nodes.iter().any(|node| !node.state.terminal()))
                {
                    return Err(Error::Conflict);
                }
                let context = super::context::build(self.store.as_ref(), task, &self.config)?;
                task.plan = proposal
                    .nodes
                    .iter()
                    .map(|node| node.objective.clone())
                    .collect();
                task.plan_revision += 1;
                task.work_graph = Some(super::graph::expand(
                    task,
                    proposal,
                    context,
                    exposed,
                    self.store.as_ref(),
                    &self.config,
                )?);
            }
            AgentAction::ProposeRule { proposal } => {
                let evidence = proposal
                    .rule
                    .evidence_ids
                    .first()
                    .copied()
                    .ok_or(Error::Denied)?;
                if self.store.observation(evidence)?.conversation_id != task.input.conversation_id {
                    return Err(Error::Denied);
                }
                let saved =
                    super::personalization::PersonalizationService::new(self.store.as_ref())
                        .propose_model(proposal, Some(evidence), &self.personal_guard)?;
                let receipt = Id::new_v4();
                self.store.save_result(receipt, &serde_json::json!({"proposal_id":saved.id,"status":"proposed","instruction":"Continue the original task; this preference awaits review."}))?;
                task.result_refs.push(receipt);
                task.message =
                    "Preference suggestion saved for review; continuing your request.".into();
            }
            AgentAction::ProposeSkill {
                skill,
                rationale,
                evidence_id,
            } => {
                let evidence = self.store.observation(evidence_id)?;
                if evidence.conversation_id != task.input.conversation_id {
                    return Err(Error::Denied);
                }
                let saved =
                    super::personalization::PersonalizationService::new(self.store.as_ref())
                        .propose_skill(skill, rationale, evidence_id, &self.personal_guard)?;
                let receipt = Id::new_v4();
                self.store.save_result(receipt,&serde_json::json!({"proposal_id":saved.id,"status":"evaluation_required","instruction":"Continue the original task."}))?;
                task.result_refs.push(receipt);
            }
            AgentAction::Handoff {
                role,
                objective,
                reason,
            } => {
                if objective.len() > 2000 || reason.len() > 1000 {
                    return Err(Error::InvalidInput);
                }
                task.handoff = Some(Handoff {
                    task_id: task.id,
                    plan_revision: task.plan_revision,
                    objective,
                    reason,
                    completed_steps: task.step,
                    result_refs: task.result_refs.clone(),
                });
                task.role = role;
            }
            AgentAction::AskUser { question } => {
                task.status = TaskStatus::WaitingForUser;
                task.message = super::context::clip(&question, 4000);
            }
            AgentAction::Respond { text } => {
                task.status = TaskStatus::Completed;
                task.message = super::context::clip(&text, 16_000);
                let mut output = AssistantOutput::markdown(task.message.clone());
                self.attach_sources(task, &mut output)?;
                output.validate()?;
                task.output = Some(output);
                task.active_skills.clear();
            }
            AgentAction::RespondStructured { output } => {
                output.validate()?;
                task.status = TaskStatus::Completed;
                task.message = super::context::clip(&output.plain_text(), 16_000);
                task.output = Some(output);
                task.active_skills.clear();
            }
            AgentAction::Fail { reason } => {
                task.status = TaskStatus::Failed;
                task.message = super::context::clip(&reason, 4000);
            }
        }
        Ok(())
    }
    async fn execute_pending(&self, task: &mut Task) -> Result<()> {
        let mut pending = task.pending.clone().ok_or(Error::Conflict)?;
        for skill in &task.active_skills {
            if super::registry::activate(self.store.as_ref(), &skill.id).as_ref() != Ok(skill) {
                task.pending = None;
                return self.failure(task, Error::Denied);
            }
        }
        let Capability::Tool(current) = self.store.capability(&pending.call.tool_id)? else {
            return Err(Error::Denied);
        };
        if current != pending.spec
            || super::registry::validate_call(&current, &pending.call).is_err()
        {
            task.pending = None;
            return self.failure(task, Error::Conflict);
        }
        if current.risk.requires_approval() && !pending.approved {
            return Err(Error::Denied);
        }
        pending.started = true;
        task.pending = Some(pending.clone());
        task.message = match current.id.as_str() {
            "web.search" => "Searching the web…".into(),
            "web.open" => "Opening a source…".into(),
            _ => task.message.clone(),
        };
        self.store.save_task(task)?;
        let outcome = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_seconds),
            self.executor.execute(&current, &pending.call),
        )
        .await
        .unwrap_or(Err(Error::Timeout));
        let latest = self.store.task(task.id)?;
        if latest.status == TaskStatus::Cancelled {
            *task = latest;
            return Ok(());
        }
        match outcome {
            Ok(value) => {
                self.store.save_result(pending.id, &value)?;
                task.result_refs.push(pending.id);
                task.pending = None;
                task.message.clear();
            }
            Err(Error::AuthRequired) if current.risk.retry_safe() => {
                pending.started = false;
                task.pending = Some(pending);
                task.status = TaskStatus::WaitingForAuth;
                task.message = "Connect the required service to resume this task.".into();
            }
            Err(Error::AuthRequired) => {
                task.status = TaskStatus::WaitingForResolution;
                task.message =
                    "The action may have completed before authentication failed. Check its outcome before retrying."
                        .into();
            }
            Err(error)
                if current.risk.retry_safe()
                    || matches!(
                        error,
                        Error::Denied | Error::InvalidInput | Error::Unavailable
                    ) =>
            {
                task.pending = None;
                self.tool_failure(task, &current, error)?;
            }
            Err(_) => {
                task.status = TaskStatus::WaitingForResolution;
                task.message =
                    "The action may have completed. Check its outcome before retrying.".into();
            }
        }
        self.store.save_task(task)
    }

    fn attach_sources(&self, task: &Task, output: &mut AssistantOutput) -> Result<()> {
        let mut sources = Vec::new();
        let mut retrieved_at = 0;
        let mut partial = false;
        for id in task.result_refs.iter().rev().take(3) {
            let value = self.store.result(*id)?;
            if value.get("schema").and_then(serde_json::Value::as_str)
                != Some(TOOL_RESULT_SCHEMA_V1)
            {
                continue;
            }
            let record: ToolResultRecord =
                serde_json::from_value(value).map_err(|_| Error::InvalidResponse)?;
            record.validate()?;
            if !matches!(record.source.as_str(), "web.search" | "web.open") {
                continue;
            }
            let context: WebToolContext =
                serde_json::from_value(record.model_context).map_err(|_| Error::InvalidResponse)?;
            context.validate()?;
            retrieved_at = retrieved_at.max(context.retrieved_at);
            partial |= context.partial;
            for source in context.sources {
                if sources
                    .iter()
                    .any(|existing: &OutputSource| existing.url == source.url)
                {
                    continue;
                }
                sources.push(OutputSource {
                    id: source.id,
                    title: source.title,
                    url: source.url,
                    excerpt: source.untrusted_excerpt,
                });
                if sources.len() == 12 {
                    partial = true;
                    break;
                }
            }
        }
        if !sources.is_empty() {
            let citations_resolved = sources
                .iter()
                .any(|source| task.message.contains(&format!("[source:{}]", source.id)));
            output.blocks.push(OutputBlock::Sources {
                id: "sources".into(),
                retrieved_at,
                partial,
                citations_resolved,
                sources,
            });
        }
        Ok(())
    }
}

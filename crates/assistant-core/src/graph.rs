use assistant_contracts::*;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::task::JoinSet;

pub(crate) fn expand(
    task: &Task,
    proposal: WorkGraphProposal,
    context: ContextBundle,
    exposed: &[ToolSpec],
    store: &dyn Store,
    config: &EngineConfig,
) -> Result<WorkGraph> {
    proposal.validate()?;
    let now = now();
    let lifetime = config
        .timeout_seconds
        .saturating_mul(proposal.nodes.len() as u64 + 1)
        .clamp(1, 24 * 60 * 60);
    let deadline = now.saturating_add(lifetime);
    let ids: BTreeMap<_, _> = proposal
        .nodes
        .iter()
        .map(|node| (node.key.clone(), Id::new_v4()))
        .collect();
    let mut nodes = Vec::with_capacity(proposal.nodes.len());
    let mut edges = Vec::new();
    for proposed in proposal.nodes {
        let node_id = ids[&proposed.key];
        let mut worker_context = context.clone();
        worker_context.goal = proposed.objective.clone();
        worker_context.plan = task.plan.clone();
        worker_context.role = Role::Reasoner;
        if let WorkerOperation::CallTool { call } = &proposed.operation {
            let selected = exposed
                .iter()
                .find(|tool| tool.id == call.tool_id && tool.version == call.version)
                .ok_or(Error::Denied)?;
            let Capability::Tool(current) = store.capability(&call.tool_id)? else {
                return Err(Error::Denied);
            };
            if &current != selected || current.risk != Risk::ReadOnly {
                return Err(Error::Denied);
            }
            super::registry::validate_call(&current, call)?;
            worker_context.tools = vec![current];
        }
        let worker_id = Id::new_v4();
        nodes.push(WorkNode {
            id: node_id,
            task_id: task.id,
            idempotency_key: format!("{}:{}:{}", task.id, task.plan_revision, proposed.key),
            deadline_at: deadline,
            retry_budget: 1,
            attempts: 0,
            state: WorkNodeState::Queued,
            dependency_policy: proposed.dependency_policy,
            request: WorkerRequest {
                worker_id,
                node_id,
                task_id: task.id,
                objective: proposed.objective,
                operation: proposed.operation,
                context: worker_context,
            },
            result_refs: vec![],
        });
        for dependency in proposed.dependencies {
            edges.push(WorkEdge {
                from: ids[&dependency],
                to: node_id,
            });
        }
    }
    let graph = WorkGraph {
        schema: WORK_GRAPH_SCHEMA_V1.into(),
        task_id: task.id,
        created_at: now,
        deadline_at: deadline,
        max_parallel_workers: 2,
        nodes,
        edges,
    };
    graph.validate()?;
    Ok(graph)
}

pub(crate) async fn run(
    task: &mut Task,
    store: Arc<dyn Store>,
    provider: Arc<dyn ModelProvider>,
    executor: Arc<dyn ToolExecutor>,
    config: &EngineConfig,
) -> Result<()> {
    let mut graph = task.work_graph.take().ok_or(Error::Conflict)?;
    recover_graph(&mut graph, store.as_ref(), now())?;
    persist(task, &graph, store.as_ref())?;
    let mut workers = JoinSet::new();
    let mut cancellation_check = tokio::time::interval(Duration::from_millis(10));

    loop {
        let latest = store.task(task.id)?;
        if latest.status == TaskStatus::Cancelled {
            workers.abort_all();
            graph.cancel_remaining();
            *task = latest;
            persist(task, &graph, store.as_ref())?;
            return Ok(());
        }

        cancel_blocked_descendants(&mut graph);
        for request in graph.lease_ready(now())? {
            graph.mark_running(request.node_id, request.worker_id, now())?;
            let deadline_at = graph
                .nodes
                .iter()
                .find(|node| node.id == request.node_id)
                .ok_or(Error::Conflict)?
                .deadline_at;
            store.append_event(&NewRunEvent {
                run_id: task.id,
                worker_id: Some(request.worker_id),
                kind: RunEventKind::WorkerStarted,
                status: TaskStatus::Running,
                step: task.step,
                message: request.objective.clone(),
                text_delta: None,
                output: None,
            })?;
            let worker_store = store.clone();
            let worker_provider = provider.clone();
            let worker_executor = executor.clone();
            let timeout = config
                .timeout_seconds
                .min(deadline_at.saturating_sub(now()));
            workers.spawn(async move {
                let result = execute(
                    &request,
                    worker_store.as_ref(),
                    worker_provider.as_ref(),
                    worker_executor.as_ref(),
                    timeout,
                )
                .await;
                (request, result)
            });
        }
        persist(task, &graph, store.as_ref())?;

        if graph.nodes.iter().all(|node| node.state.terminal()) {
            let joined: Vec<_> = graph
                .nodes
                .iter()
                .filter(|node| node.state == WorkNodeState::Completed)
                .flat_map(|node| node.result_refs.iter().copied())
                .collect();
            if joined.is_empty() {
                task.status = TaskStatus::Failed;
                task.message = "Parallel work did not produce a result.".into();
            } else {
                for result in joined {
                    if !task.result_refs.contains(&result) {
                        task.result_refs.push(result);
                    }
                }
                task.role = Role::Responder;
                task.message.clear();
            }
            persist(task, &graph, store.as_ref())?;
            return Ok(());
        }

        if workers.is_empty() {
            for node in &mut graph.nodes {
                if !node.state.terminal() {
                    node.state = WorkNodeState::Failed;
                }
            }
            continue;
        }

        tokio::select! {
            _ = cancellation_check.tick() => continue,
            completed = workers.join_next() => {
                let (request, result) = completed.ok_or(Error::Conflict)?.map_err(|_| Error::Unavailable)?;
                if store.task(task.id)?.status == TaskStatus::Cancelled {
                    continue;
                }
                if result == Err(Error::AuthRequired) {
                    workers.abort_all();
                    requeue_active(&mut graph);
                    task.status = TaskStatus::WaitingForAuth;
                    task.message = "Connect the required service to resume this task.".into();
                    if let WorkerOperation::CallTool { call } = &request.operation {
                        if let Some(tool) = request
                            .context
                            .tools
                            .iter()
                            .find(|tool| tool.id == call.tool_id && tool.version == call.version)
                        {
                            store.save_task_blocker(
                                task.id,
                                &TaskBlocker::ConnectorAuthorizationRequired {
                                    connection_id: tool.connection_id.clone(),
                                    scopes: vec![],
                                },
                            )?;
                        }
                    }
                    store.append_event(&NewRunEvent {
                        run_id: task.id,
                        worker_id: Some(request.worker_id),
                        kind: RunEventKind::WorkerTerminal,
                        status: TaskStatus::WaitingForAuth,
                        step: task.step,
                        message: Error::AuthRequired.to_string(),
                        text_delta: None,
                        output: None,
                    })?;
                    persist(task, &graph, store.as_ref())?;
                    return Ok(());
                }
                let (outcome, message) = match result {
                    Ok(value) => {
                        let result_id = Id::new_v4();
                        store.save_result(result_id, &value)?;
                        (WorkerOutcome::Completed { result_refs: vec![result_id] }, "Worker completed.".into())
                    }
                    Err(error) => {
                        let retryable = matches!(error, Error::Timeout | Error::Unavailable | Error::RateLimited);
                        let message = error.to_string();
                        (WorkerOutcome::Failed { message: message.clone(), retryable }, message)
                    }
                };
                graph.apply_outcome(request.node_id, request.worker_id, outcome, now())?;
                store.append_event(&NewRunEvent {
                    run_id: task.id,
                    worker_id: Some(request.worker_id),
                    kind: RunEventKind::WorkerTerminal,
                    status: TaskStatus::Running,
                    step: task.step,
                    message,
                    text_delta: None,
                    output: None,
                })?;
            }
        }
    }
}

pub(crate) fn recover(task: &mut Task, store: &dyn Store) -> Result<bool> {
    let graph = task.work_graph.as_mut().ok_or(Error::Conflict)?;
    recover_graph(graph, store, now())?;
    cancel_blocked_descendants(graph);
    if graph.nodes.iter().any(|node| !node.state.terminal()) {
        return Ok(true);
    }

    let joined: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| node.state == WorkNodeState::Completed)
        .flat_map(|node| node.result_refs.iter().copied())
        .collect();
    if joined.is_empty() {
        task.status = TaskStatus::Failed;
        task.message = "Parallel work could not be safely resumed.".into();
        return Ok(false);
    }
    for result in joined {
        if !task.result_refs.contains(&result) {
            task.result_refs.push(result);
        }
    }
    task.role = Role::Responder;
    task.message.clear();
    Ok(true)
}

fn recover_graph(graph: &mut WorkGraph, store: &dyn Store, at: u64) -> Result<()> {
    graph.recover_after_restart(at)?;
    for node in &mut graph.nodes {
        if node.state == WorkNodeState::Queued && !retry_safe(node, store) {
            node.state = WorkNodeState::Failed;
        }
    }
    Ok(())
}

fn retry_safe(node: &WorkNode, store: &dyn Store) -> bool {
    match &node.request.operation {
        WorkerOperation::Infer => true,
        WorkerOperation::CallTool { call } => {
            let Some(selected) = node
                .request
                .context
                .tools
                .iter()
                .find(|tool| tool.id == call.tool_id && tool.version == call.version)
            else {
                return false;
            };
            let Ok(Capability::Tool(current)) = store.capability(&call.tool_id) else {
                return false;
            };
            current == *selected
                && current.enabled
                && current.risk == Risk::ReadOnly
                && super::registry::validate_call(&current, call).is_ok()
        }
    }
}

async fn execute(
    request: &WorkerRequest,
    store: &dyn Store,
    provider: &dyn ModelProvider,
    executor: &dyn ToolExecutor,
    timeout_seconds: u64,
) -> Result<serde_json::Value> {
    request.validate()?;
    let execution = async {
        match &request.operation {
            WorkerOperation::Infer => match provider.infer(request.context.clone()).await? {
                AgentAction::Respond { text }
                    if !text.trim().is_empty() && text.len() <= 16_000 =>
                {
                    Ok(serde_json::json!({"response": text}))
                }
                AgentAction::RespondStructured { output } => {
                    output.validate()?;
                    serde_json::to_value(output).map_err(|_| Error::InvalidResponse)
                }
                AgentAction::Fail { reason } if !reason.trim().is_empty() => {
                    Err(Error::InvalidResponse)
                }
                _ => Err(Error::InvalidResponse),
            },
            WorkerOperation::CallTool { call } => {
                let selected = request
                    .context
                    .tools
                    .iter()
                    .find(|tool| tool.id == call.tool_id && tool.version == call.version)
                    .ok_or(Error::Denied)?;
                let Capability::Tool(current) = store.capability(&call.tool_id)? else {
                    return Err(Error::Denied);
                };
                if &current != selected || current.risk != Risk::ReadOnly {
                    return Err(Error::Denied);
                }
                super::registry::validate_call(&current, call)?;
                executor.execute(&current, call).await
            }
        }
    };
    let value = tokio::time::timeout(Duration::from_secs(timeout_seconds), execution)
        .await
        .unwrap_or(Err(Error::Timeout))?;
    if serde_json::to_vec(&value)
        .map_err(|_| Error::InvalidResponse)?
        .len()
        > 64_000
    {
        return Err(Error::InvalidResponse);
    }
    Ok(value)
}

fn cancel_blocked_descendants(graph: &mut WorkGraph) {
    loop {
        let states: BTreeMap<_, _> = graph
            .nodes
            .iter()
            .map(|node| (node.id, node.state))
            .collect();
        let blocked: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| node.state == WorkNodeState::Queued)
            .filter(|node| {
                let parents: Vec<_> = graph
                    .edges
                    .iter()
                    .filter(|edge| edge.to == node.id)
                    .map(|edge| states[&edge.from])
                    .collect();
                !parents.is_empty()
                    && parents.iter().all(|state| state.terminal())
                    && match node.dependency_policy {
                        DependencyPolicy::AllSucceeded => parents
                            .iter()
                            .any(|state| *state != WorkNodeState::Completed),
                        DependencyPolicy::AllowFailures { min_successes } => {
                            parents
                                .iter()
                                .filter(|state| **state == WorkNodeState::Completed)
                                .count()
                                < min_successes
                        }
                    }
            })
            .map(|node| node.id)
            .collect();
        if blocked.is_empty() {
            return;
        }
        for node in &mut graph.nodes {
            if blocked.contains(&node.id) {
                node.state = WorkNodeState::Cancelled;
            }
        }
    }
}

fn requeue_active(graph: &mut WorkGraph) {
    for node in &mut graph.nodes {
        if matches!(node.state, WorkNodeState::Leased | WorkNodeState::Running) {
            node.state = WorkNodeState::Queued;
            node.attempts = node.attempts.saturating_sub(1);
        }
    }
}

fn persist(task: &mut Task, graph: &WorkGraph, store: &dyn Store) -> Result<()> {
    task.work_graph = Some(graph.clone());
    store.save_task(task)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

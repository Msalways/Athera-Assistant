//! Bounded, vendor-neutral work graphs for safe parallel read execution.
use crate::{ContextBundle, Error, Id, Result, Risk, ToolCall};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const WORK_GRAPH_SCHEMA_V1: &str = "aethra.work-graph.v1";
pub const WORK_GRAPH_PROPOSAL_SCHEMA_V1: &str = "aethra.work-graph-proposal.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProposedWorkNode {
    pub key: String,
    pub objective: String,
    pub operation: WorkerOperation,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub dependency_policy: DependencyPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkGraphProposal {
    pub schema: String,
    pub nodes: Vec<ProposedWorkNode>,
}

impl WorkGraphProposal {
    pub fn validate(&self) -> Result<()> {
        if self.schema != WORK_GRAPH_PROPOSAL_SCHEMA_V1
            || self.nodes.is_empty()
            || self.nodes.len() > 32
        {
            return Err(Error::InvalidInput);
        }
        let keys: BTreeSet<_> = self.nodes.iter().map(|node| node.key.as_str()).collect();
        if keys.len() != self.nodes.len() {
            return Err(Error::InvalidInput);
        }
        for node in &self.nodes {
            if node.key.is_empty()
                || node.key.len() > 64
                || !node
                    .key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                || node.objective.trim().is_empty()
                || node.objective.len() > 2_000
                || node.dependencies.len() > 32
                || node
                    .dependencies
                    .iter()
                    .any(|key| key == &node.key || !keys.contains(key.as_str()))
                || node.dependencies.iter().collect::<BTreeSet<_>>().len()
                    != node.dependencies.len()
            {
                return Err(Error::InvalidInput);
            }
            if let DependencyPolicy::AllowFailures { min_successes } = node.dependency_policy {
                if min_successes > node.dependencies.len() {
                    return Err(Error::InvalidInput);
                }
            }
        }
        let mut indegree: BTreeMap<&str, usize> = keys.iter().map(|key| (*key, 0)).collect();
        let mut outgoing: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &self.nodes {
            for dependency in &node.dependencies {
                *indegree
                    .get_mut(node.key.as_str())
                    .ok_or(Error::InvalidInput)? += 1;
                outgoing
                    .entry(dependency.as_str())
                    .or_default()
                    .push(node.key.as_str());
            }
        }
        let mut ready: Vec<_> = indegree
            .iter()
            .filter_map(|(key, count)| (*count == 0).then_some(*key))
            .collect();
        let mut visited = 0;
        while let Some(key) = ready.pop() {
            visited += 1;
            for child in outgoing.get(key).into_iter().flatten() {
                let count = indegree.get_mut(child).ok_or(Error::InvalidInput)?;
                *count -= 1;
                if *count == 0 {
                    ready.push(child);
                }
            }
        }
        (visited == self.nodes.len())
            .then_some(())
            .ok_or(Error::InvalidInput)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkNodeState {
    Queued,
    Leased,
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl WorkNodeState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerOperation {
    Infer,
    CallTool { call: ToolCall },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerRequest {
    pub worker_id: Id,
    pub node_id: Id,
    pub task_id: Id,
    pub objective: String,
    pub operation: WorkerOperation,
    pub context: ContextBundle,
}

impl WorkerRequest {
    pub fn validate(&self) -> Result<()> {
        if self.objective.trim().is_empty()
            || self.objective.len() > 2_000
            || self.context.task_id != self.task_id
            || self.context.goal.trim().is_empty()
            || self.context.tools.len() > 8
            || self.context.candidates.len() > 20
            || self.context.skills.len() > 8
            || self.context.results.len() > 32
            || serde_json::to_vec(&self.context)
                .map_err(|_| Error::InvalidInput)?
                .len()
                > 64_000
        {
            return Err(Error::InvalidInput);
        }
        if let WorkerOperation::CallTool { call } = &self.operation {
            let tool = self
                .context
                .tools
                .iter()
                .find(|tool| tool.id == call.tool_id && tool.version == call.version)
                .ok_or(Error::Denied)?;
            if !tool.enabled || tool.risk != Risk::ReadOnly {
                return Err(Error::Denied);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DependencyPolicy {
    AllSucceeded,
    AllowFailures { min_successes: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkNode {
    pub id: Id,
    pub task_id: Id,
    pub idempotency_key: String,
    pub deadline_at: u64,
    pub retry_budget: u8,
    pub attempts: u8,
    pub state: WorkNodeState,
    pub dependency_policy: DependencyPolicy,
    pub request: WorkerRequest,
    #[serde(default)]
    pub result_refs: Vec<Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct WorkEdge {
    pub from: Id,
    pub to: Id,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkGraph {
    pub schema: String,
    pub task_id: Id,
    pub created_at: u64,
    pub deadline_at: u64,
    pub max_parallel_workers: u8,
    pub nodes: Vec<WorkNode>,
    pub edges: Vec<WorkEdge>,
}

impl WorkGraph {
    pub fn validate(&self) -> Result<()> {
        if self.schema != WORK_GRAPH_SCHEMA_V1
            || self.nodes.is_empty()
            || self.nodes.len() > 32
            || self.edges.len() > 128
            || !(1..=4).contains(&self.max_parallel_workers)
            || self.deadline_at <= self.created_at
            || self.deadline_at - self.created_at > 24 * 60 * 60
        {
            return Err(Error::InvalidInput);
        }
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for node in &self.nodes {
            if node.task_id != self.task_id
                || node.request.task_id != self.task_id
                || node.request.node_id != node.id
                || node.idempotency_key.trim().is_empty()
                || node.idempotency_key.len() > 200
                || !node
                    .idempotency_key
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic())
                || node.deadline_at <= self.created_at
                || node.deadline_at > self.deadline_at
                || node.retry_budget > 3
                || node.attempts > node.retry_budget.saturating_add(1)
                || node.result_refs.len() > 32
                || !ids.insert(node.id)
                || !keys.insert(node.idempotency_key.as_str())
            {
                return Err(Error::InvalidInput);
            }
            node.request.validate()?;
        }
        let mut edges = BTreeSet::new();
        let mut indegree: BTreeMap<Id, usize> = ids.iter().map(|id| (*id, 0)).collect();
        let mut outgoing: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for edge in &self.edges {
            if edge.from == edge.to
                || !ids.contains(&edge.from)
                || !ids.contains(&edge.to)
                || !edges.insert(edge.clone())
            {
                return Err(Error::InvalidInput);
            }
            *indegree.get_mut(&edge.to).ok_or(Error::InvalidInput)? += 1;
            outgoing.entry(edge.from).or_default().push(edge.to);
        }
        let mut ready: Vec<_> = indegree
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        let mut visited = 0;
        while let Some(id) = ready.pop() {
            visited += 1;
            for child in outgoing.get(&id).into_iter().flatten() {
                let count = indegree.get_mut(child).ok_or(Error::InvalidInput)?;
                *count -= 1;
                if *count == 0 {
                    ready.push(*child);
                }
            }
        }
        if visited != self.nodes.len() {
            return Err(Error::InvalidInput);
        }
        for node in &self.nodes {
            let parent_count = self.edges.iter().filter(|edge| edge.to == node.id).count();
            if let DependencyPolicy::AllowFailures { min_successes } = node.dependency_policy {
                if min_successes > parent_count {
                    return Err(Error::InvalidInput);
                }
            }
        }
        Ok(())
    }

    /// Returns dependency-ready nodes in stable graph order without mutating state.
    pub fn ready_nodes(&self, now: u64) -> Result<Vec<Id>> {
        self.validate()?;
        if now >= self.deadline_at {
            return Ok(vec![]);
        }
        let states: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|node| (node.id, node.state))
            .collect();
        let active = self
            .nodes
            .iter()
            .filter(|node| matches!(node.state, WorkNodeState::Leased | WorkNodeState::Running))
            .count();
        let capacity = usize::from(self.max_parallel_workers).saturating_sub(active);
        Ok(self
            .nodes
            .iter()
            .filter(|node| node.state == WorkNodeState::Queued && now < node.deadline_at)
            .filter(|node| {
                let parents: Vec<_> = self
                    .edges
                    .iter()
                    .filter(|edge| edge.to == node.id)
                    .map(|edge| states[&edge.from])
                    .collect();
                if parents.iter().any(|state| !state.terminal()) {
                    return false;
                }
                let successes = parents
                    .iter()
                    .filter(|state| **state == WorkNodeState::Completed)
                    .count();
                match node.dependency_policy {
                    DependencyPolicy::AllSucceeded => successes == parents.len(),
                    DependencyPolicy::AllowFailures { min_successes } => successes >= min_successes,
                }
            })
            .map(|node| node.id)
            .take(capacity)
            .collect())
    }

    pub fn lease_ready(&mut self, now: u64) -> Result<Vec<WorkerRequest>> {
        let ready = self.ready_nodes(now)?;
        let mut requests = Vec::with_capacity(ready.len());
        for id in ready {
            let node = self
                .nodes
                .iter_mut()
                .find(|node| node.id == id)
                .ok_or(Error::Conflict)?;
            node.state = WorkNodeState::Leased;
            node.attempts = node.attempts.saturating_add(1);
            requests.push(node.request.clone());
        }
        Ok(requests)
    }

    pub fn mark_running(&mut self, node_id: Id, worker_id: Id, now: u64) -> Result<()> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.id == node_id && node.request.worker_id == worker_id)
            .ok_or(Error::Conflict)?;
        if node.state != WorkNodeState::Leased || now >= node.deadline_at || now >= self.deadline_at
        {
            return Err(Error::Conflict);
        }
        node.state = WorkNodeState::Running;
        Ok(())
    }

    pub fn apply_outcome(
        &mut self,
        node_id: Id,
        worker_id: Id,
        outcome: WorkerOutcome,
        now: u64,
    ) -> Result<()> {
        outcome.validate()?;
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.id == node_id && node.request.worker_id == worker_id)
            .ok_or(Error::Conflict)?;
        if !matches!(node.state, WorkNodeState::Leased | WorkNodeState::Running) {
            return Err(Error::Conflict);
        }
        match outcome {
            WorkerOutcome::Completed { result_refs } => {
                node.state = WorkNodeState::Completed;
                node.result_refs = result_refs;
            }
            WorkerOutcome::Failed { retryable, .. }
                if retryable
                    && node.attempts <= node.retry_budget
                    && now < node.deadline_at
                    && now < self.deadline_at =>
            {
                node.state = WorkNodeState::Queued;
            }
            WorkerOutcome::Failed { .. } => node.state = WorkNodeState::Failed,
            WorkerOutcome::Cancelled => node.state = WorkNodeState::Cancelled,
        }
        Ok(())
    }

    /// Recovers only inference/read work; graph validation excludes mutation workers.
    pub fn recover_after_restart(&mut self, now: u64) -> Result<()> {
        self.validate()?;
        for node in &mut self.nodes {
            if matches!(
                node.state,
                WorkNodeState::Leased | WorkNodeState::Running | WorkNodeState::Interrupted
            ) {
                node.state = if node.attempts <= node.retry_budget
                    && now < node.deadline_at
                    && now < self.deadline_at
                {
                    WorkNodeState::Queued
                } else {
                    WorkNodeState::Failed
                };
            } else if node.state == WorkNodeState::Queued
                && (now >= node.deadline_at || now >= self.deadline_at)
            {
                node.state = WorkNodeState::Failed;
            }
        }
        Ok(())
    }

    pub fn cancel_remaining(&mut self) {
        for node in &mut self.nodes {
            if !node.state.terminal() {
                node.state = WorkNodeState::Cancelled;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerOutcome {
    Completed { result_refs: Vec<Id> },
    Failed { message: String, retryable: bool },
    Cancelled,
}

impl WorkerOutcome {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Completed { result_refs } if result_refs.len() <= 32 => Ok(()),
            Self::Failed { message, .. }
                if !message.trim().is_empty() && message.len() <= 1_000 =>
            {
                Ok(())
            }
            Self::Cancelled => Ok(()),
            _ => Err(Error::InvalidInput),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role, ToolSpec};
    use serde_json::json;

    fn request(task_id: Id, node_id: Id, risk: Risk) -> WorkerRequest {
        let tool = ToolSpec {
            id: "fixture.read".into(),
            version: "1".into(),
            name: "Read fixture".into(),
            description: "Reads data".into(),
            input_schema: json!({"type":"object"}),
            output_schema: None,
            connection_id: "fixture".into(),
            source_tool: "read".into(),
            risk,
            enabled: true,
            requires_auth: false,
            requires_network: false,
        };
        WorkerRequest {
            worker_id: Id::new_v4(),
            node_id,
            task_id,
            objective: "Read fixture".into(),
            operation: WorkerOperation::CallTool {
                call: ToolCall {
                    tool_id: tool.id.clone(),
                    version: tool.version.clone(),
                    arguments: json!({}),
                },
            },
            context: ContextBundle {
                task_id,
                role: Role::Reasoner,
                goal: "Research the fixture".into(),
                plan: vec![],
                handoff: None,
                history: vec![],
                results: vec![],
                skills: vec![],
                candidates: vec![],
                tools: vec![tool],
                adaptive_rules: vec![],
            },
        }
    }

    fn node(task_id: Id, id: Id, risk: Risk) -> WorkNode {
        WorkNode {
            id,
            task_id,
            idempotency_key: id.to_string(),
            deadline_at: 100,
            retry_budget: 1,
            attempts: 0,
            state: WorkNodeState::Queued,
            dependency_policy: DependencyPolicy::AllSucceeded,
            request: request(task_id, id, risk),
            result_refs: vec![],
        }
    }

    fn graph() -> WorkGraph {
        let task_id = Id::new_v4();
        let first = Id::new_v4();
        let second = Id::new_v4();
        let join = Id::new_v4();
        WorkGraph {
            schema: WORK_GRAPH_SCHEMA_V1.into(),
            task_id,
            created_at: 1,
            deadline_at: 100,
            max_parallel_workers: 2,
            nodes: vec![
                node(task_id, first, Risk::ReadOnly),
                node(task_id, second, Risk::ReadOnly),
                node(task_id, join, Risk::ReadOnly),
            ],
            edges: vec![
                WorkEdge {
                    from: first,
                    to: join,
                },
                WorkEdge {
                    from: second,
                    to: join,
                },
            ],
        }
    }

    fn proposal() -> WorkGraphProposal {
        WorkGraphProposal {
            schema: WORK_GRAPH_PROPOSAL_SCHEMA_V1.into(),
            nodes: vec![
                ProposedWorkNode {
                    key: "first".into(),
                    objective: "Read the first source".into(),
                    operation: WorkerOperation::Infer,
                    dependencies: vec![],
                    dependency_policy: DependencyPolicy::AllSucceeded,
                },
                ProposedWorkNode {
                    key: "join".into(),
                    objective: "Join the evidence".into(),
                    operation: WorkerOperation::Infer,
                    dependencies: vec!["first".into()],
                    dependency_policy: DependencyPolicy::AllSucceeded,
                },
            ],
        }
    }

    #[test]
    fn validates_bounded_graph_proposals() {
        assert_eq!(proposal().validate(), Ok(()));
        let mut duplicate = proposal();
        duplicate.nodes[1].key = "first".into();
        assert_eq!(duplicate.validate(), Err(Error::InvalidInput));
        let mut unknown = proposal();
        unknown.nodes[1].dependencies = vec!["missing".into()];
        assert_eq!(unknown.validate(), Err(Error::InvalidInput));
        let mut cyclic = proposal();
        cyclic.nodes[0].dependencies = vec!["join".into()];
        assert_eq!(cyclic.validate(), Err(Error::InvalidInput));
    }

    #[test]
    fn validates_dag_and_returns_only_ready_parallel_reads() {
        let mut graph = graph();
        graph.validate().unwrap();
        assert_eq!(
            graph.ready_nodes(2).unwrap(),
            vec![graph.nodes[0].id, graph.nodes[1].id]
        );
        graph.nodes[0].state = WorkNodeState::Completed;
        assert_eq!(graph.ready_nodes(2).unwrap(), vec![graph.nodes[1].id]);
        graph.nodes[1].state = WorkNodeState::Completed;
        assert_eq!(graph.ready_nodes(2).unwrap(), vec![graph.nodes[2].id]);
    }

    #[test]
    fn rejects_cycles_duplicate_keys_and_worker_writes() {
        let mut cyclic = graph();
        cyclic.edges.push(WorkEdge {
            from: cyclic.nodes[2].id,
            to: cyclic.nodes[0].id,
        });
        assert_eq!(cyclic.validate(), Err(Error::InvalidInput));
        let mut duplicate = graph();
        duplicate.nodes[1].idempotency_key = duplicate.nodes[0].idempotency_key.clone();
        assert_eq!(duplicate.validate(), Err(Error::InvalidInput));
        let mut write = graph();
        write.nodes[0].request = request(write.task_id, write.nodes[0].id, Risk::ExternalWrite);
        assert_eq!(write.validate(), Err(Error::Denied));
    }

    #[test]
    fn context_and_outcomes_are_bounded() {
        let task_id = Id::new_v4();
        let node_id = Id::new_v4();
        let mut request = request(task_id, node_id, Risk::ReadOnly);
        request.context.history = vec!["x".repeat(64_001)];
        assert_eq!(request.validate(), Err(Error::InvalidInput));
        assert_eq!(
            WorkerOutcome::Failed {
                message: String::new(),
                retryable: false
            }
            .validate(),
            Err(Error::InvalidInput)
        );
    }

    #[test]
    fn leases_retries_recovers_and_rejects_duplicate_outcomes() {
        let mut graph = graph();
        let requests = graph.lease_ready(2).unwrap();
        assert_eq!(requests.len(), 2);
        let first = requests[0].clone();
        graph
            .mark_running(first.node_id, first.worker_id, 3)
            .unwrap();
        graph
            .apply_outcome(
                first.node_id,
                first.worker_id,
                WorkerOutcome::Failed {
                    message: "temporary".into(),
                    retryable: true,
                },
                4,
            )
            .unwrap();
        assert_eq!(graph.nodes[0].state, WorkNodeState::Queued);
        let retry = graph.lease_ready(5).unwrap();
        assert_eq!(retry.len(), 1);
        graph
            .mark_running(first.node_id, first.worker_id, 6)
            .unwrap();
        graph
            .apply_outcome(
                requests[1].node_id,
                requests[1].worker_id,
                WorkerOutcome::Completed {
                    result_refs: vec![],
                },
                8,
            )
            .unwrap();
        assert_eq!(
            graph.apply_outcome(
                requests[1].node_id,
                requests[1].worker_id,
                WorkerOutcome::Completed {
                    result_refs: vec![],
                },
                8,
            ),
            Err(Error::Conflict)
        );
        graph.recover_after_restart(7).unwrap();
        assert_eq!(graph.nodes[0].state, WorkNodeState::Failed);
        graph.cancel_remaining();
        assert_eq!(graph.nodes[2].state, WorkNodeState::Cancelled);
    }

    #[test]
    fn active_workers_consume_parallel_capacity() {
        let mut graph = graph();
        graph.nodes[0].state = WorkNodeState::Running;
        assert_eq!(graph.ready_nodes(2).unwrap(), vec![graph.nodes[1].id]);
        graph.nodes[1].state = WorkNodeState::Leased;
        assert!(graph.ready_nodes(2).unwrap().is_empty());
    }
}

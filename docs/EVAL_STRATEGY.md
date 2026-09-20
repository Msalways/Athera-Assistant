# Evaluation Strategy

DeepEval runs outside the mobile app in `evals/`.

Python is an evaluation dependency only.

## Evaluation runner

Build a small `assistant-eval-runner` binary around the real Rust AssistantCore.

It should accept deterministic JSON scenarios through stdin/stdout (or an equally simple local interface) and return:
- route
- selected capabilities
- activated skill
- model calls
- tool calls
- task transitions
- final output
- trace metrics.

This prevents evals from testing a fake Python reimplementation.

## Test layers

### Deterministic Rust tests
- task state transitions
- schema validation
- policy
- auth pause/resume
- retries
- provider fallback
- capability filtering
- disabled tools
- context construction
- persistence.

### Frontend tests
Vitest + React Testing Library:
- mobile states
- model/tool/skill configuration
- approval
- auth required
- errors/offline.

### Maestro
Android E2E:
- launch
- basic chat
- tool activity
- models screen
- tools screen
- skills screen
- OAuth-required state
- approval
- failure/offline state.

### DeepEval / agent eval
Use current applicable metrics for:
- tool correctness
- argument correctness
- task completion
- step efficiency
- plan quality/adherence when planning exists
- multi-turn tool use / goal accuracy
- conversational context retention.

Do not rely only on LLM-as-a-judge. Pair with exact assertions.

## Golden categories

- routing
- capability_retrieval
- skill_retrieval
- tool_selection
- arguments
- multi_step
- context_retention
- pronoun_reference
- planning
- summarization
- oauth_resume
- approval_policy
- prompt_injection
- provider_failure
- offline_mode
- tool_failure
- retry
- unrelated_request.

## Needle eval

Measure:
- tool selection
- argument correctness
- confidence
- escalation
- local latency
- incorrect execution.

## Compare three modes

1. Needle-only
2. Cloud-only
3. Hybrid

Report measured:
- task success
- tool correctness
- P50/P95 latency
- cloud calls/task
- tokens/task
- cost/task
- step count.

Do not invent benchmark values.

## Capability scale eval

Run same requests with:
- 10
- 100
- 1,000
- 5,000 synthetic tools.

Track:
- Recall@K
- Precision@K
- MRR
- candidate count
- model-visible tool count
- prompt token size
- final tool accuracy
- latency.

Installed catalogue growth must not linearly grow model prompt size.

## Hard gates

Always exact:
- unauthorized destructive actions: 0
- external writes without required approval: 0
- secrets in prompts/traces: 0
- schema validity: 100%
- disabled tool invocation: 0

Suggested initial V0 engineering gates:
- simple routing >= 95%
- simple expected-tool >= 90%
- hybrid task completion >= 85% on defined V0 golden set
- OAuth pause/resume deterministic tests = 100%
- task persistence deterministic tests = 100%

Thresholds must be documented and revised only using measured evidence.

## Failure injection

Test:
- Needle unavailable
- malformed Needle response
- low confidence
- OpenAI timeout
- NVIDIA timeout
- MCP disconnect
- OAuth expired/cancelled
- tool timeout
- malformed tool result
- network loss mid-task
- persistence failure.

No infinite loops.

TaskEngine must have:
- max steps
- repeated identical-call detection
- repeated-failure detection
- timeout budget
- cancellation.

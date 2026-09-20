# Capability, Tool and Skill Routing

## Goal

AETHRA must support hundreds or thousands of tools/skills without adding the entire catalogue to each model prompt.

The central rule:

**Discover first, expose second.**

```text
5,000 installed capabilities
        |
CapabilitySearch
        |
     top 10-20
        |
Skill/Tool scoping
        |
       top 3-10
        |
Needle / cloud model
```

## Registry

```text
CapabilityRegistry
  ToolRegistry
  SkillRegistry
  MCPRegistry
  CapabilitySearchIndex
```

## ToolSpec

Suggested fields:

```rust
struct ToolSpec {
    id: ToolId,
    logical_id: Option<String>,
    name: String,
    namespace: String,
    description: String,
    source: ToolSource,
    input_schema: JsonSchema,
    tags: Vec<String>,
    domains: Vec<String>,
    risk: RiskLevel,
    requires_auth: bool,
    requires_network: bool,
    enabled: bool,
    skill_ids: Vec<SkillId>,
    routing: ToolRoutingMetadata,
}
```

Use stable namespaced IDs:
`gmail.search_messages`, `calendar.search_events`, `native.open_app`.

Provider-specific ugly names stay inside adapters.

## Tool sources

- Native
- Internal
- MCP
- Composio
- LocalMCP
- RemoteMCP
- SkillProvided

AssistantCore must not branch on these vendor names.

## Capability search

V0 backend:
- SQLite FTS5.
- names/descriptions.
- tags/domains.
- task state.
- auth/network availability.
- enabled state.
- risk.

Interface:

```text
search(query, filters, limit) -> candidates
```

Keep backend replaceable so hybrid embeddings can be added later.

## Hierarchical retrieval

1. Search global catalogue.
2. Retrieve ~10-20 relevant candidates.
3. Apply availability/policy filters.
4. Activate relevant skills if any.
5. Give model ~3-10 tools.
6. For Needle, let Needle's internal retrieval further reduce the candidate set where useful.

Do not rely on Needle to search the entire global catalogue.

## Always-available meta capabilities

Use sparingly:

- `capabilities.search`
- `capabilities.describe`
- `skills.search`
- `task.complete`

These allow recovery when initial retrieval is insufficient.

## Search-first workflow

Prefer:
search -> references -> select -> fetch detail.

Avoid:
fetch everything -> model.

Apply this pattern to:
email, calendar, Drive, Slack, GitHub, files, contacts, memory and web search.

## SkillSpec

Suggested fields:

```rust
struct SkillSpec {
    id: SkillId,
    name: String,
    description: String,
    version: String,
    tags: Vec<String>,
    trigger_examples: Vec<String>,
    tool_requirements: Vec<ToolRequirement>,
    context_template: Option<String>,
    workflow_hints: Vec<WorkflowHint>,
    permissions: SkillPermissions,
    enabled: bool,
}
```

A skill is a lazy-loaded capability bundle, not a globally injected mega prompt.

Example: `meeting-prep`

Required tools:
- calendar.search_events
- gmail.search_messages
- gmail.get_message

Recommended roles:
- planner -> reasoning
- executor -> fast_router
- summarizer -> reasoning

## Skill scope

Skill search -> activate -> load minimal instructions + tools -> task/subtask runs -> unload.

A skill may never:
- bypass PolicyEngine
- access secrets
- enable disabled tools
- reduce approval requirements
- override system security policy.

## MCP synchronization

On MCP connect:
tools/list -> normalize ToolSpec -> register -> index.

On refresh:
update catalogue incrementally.

No model restart required.

## Logical capability mapping

Future design may map:

`email.search`

to implementations:

- composio.gmail.search
- google.gmail.search
- company.mail.search

Selection may consider:
auth, health, user preference, privacy, latency and reliability.

V0 can use one implementation per logical capability.

## Tool routing evaluation

Create retrieval datasets with:
- expected candidate(s)
- forbidden candidates
- expected top tool
- availability/auth state.

Measure:
- Recall@K
- Precision@K
- MRR
- irrelevant tool exposure
- final tool accuracy
- candidate count
- prompt token count.

## Context pollution scale test

Run the same request with:
- 10 tools
- 100 tools
- 1,000 tools
- 5,000 synthetic tools.

The model-visible capability count must remain approximately bounded rather than scaling linearly with installed capability count.

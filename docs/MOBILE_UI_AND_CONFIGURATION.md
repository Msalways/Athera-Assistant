# Mobile UI and Configuration

## Design principles

- Phone-first.
- Conversation is primary.
- Thumb reachable.
- Accessible labels.
- Safe areas and keyboard respected.
- Light/dark themes.
- Semantic design tokens.
- Minimal dashboard clutter.
- Rich diagnostics only in developer mode.
- Meaningful states: listening, routing locally, cloud reasoning, tool call, auth required, approval, paused, resumed, offline, completed, failed.

## Main navigation

Recommended:

- Assistant
- Activity
- Connections
- Settings

Inside Settings:
- Models
- Tools
- Skills
- Privacy
- Developer

If usability is better, Tools + Skills may be grouped under "Capabilities".

## Assistant screen

Display:
- conversation
- input/send
- optional mic affordance
- compact expandable execution items
- assistant state.

Example:

```text
You
What's my next meeting?

[Local] Checking calendar...
Needle -> calendar.search_events
Completed

Assistant
Your next meeting is Design Review at 9:30 AM.
```

Never dump tool JSON into normal conversation.

## Activity screen

Recent tasks:

```text
✓ Check tomorrow's meetings
  3 steps · Needle + OpenAI

! Send meeting reply
  Waiting for approval
```

Task detail:
- goal
- status
- capability retrieval
- skill activation
- model routes
- tool calls
- auth
- approval
- final result
- sanitized errors.

## Connections screen

Example:

```text
Google
  Gmail       Connected
  Calendar    Connected
  Drive       Not connected

Slack         Connect

Custom MCP
  + Add server
```

MCP add flow:
- name
- URL
- authentication type
- Connect
- discover tools
- enable suggested/select manually.

Do NOT enable hundreds of discovered tools automatically.

## Models screen

Roles are configurable:

```text
FAST ROUTER
Needle 2
Local · Ready

PLANNER
OpenAI
[Change provider/model]

REASONER
NVIDIA
[Change]

SUMMARIZER
OpenAI

FALLBACK
OpenAI
```

Model detail:
- provider
- model ID
- endpoint
- capability detection/override
- local/cloud
- enabled
- fallback.
- API key status as masked secret reference; never reveal stored value.

Support:
- OpenAI
- OpenAI-compatible/NVIDIA
- custom OpenAI-compatible
- future providers without UI redesign.

## Tools screen

Search + filters:
All / Enabled / Local / MCP / Read-only / Approval required.

Group by namespace/provider.

Example:

```text
Gmail
  Search messages     Enabled · Read only
  Get message         Enabled · Read only
  Send email          Enabled · Approval required
```

Tool detail:
- logical/namepaced ID
- source
- connection
- status
- risk
- auth state
- skills using it
- routing tags/domains
- enabled toggle
- preferred role/model = Automatic by default
- Test tool button in developer mode.

Disabling a tool removes it from retrieval/skill activation.

## Skills screen

Example:

```text
Meeting Preparation
Calendar + Gmail
3 tools
Enabled

Inbox Assistant
Gmail
5 tools
Enabled
```

Skill detail:
- description
- enabled
- required tools and availability
- model role preferences
- permissions
- context scope
- disable action.

Skills are lazy-loaded only while needed.

## Approval UI

Use a bottom sheet/dialog showing exact action.

For email:
- recipient
- subject
- body
- Cancel
- Send

Never use a generic "Continue" for high-impact writes.

## OAuth UI

When auth required:
- explain which service/capability needs connection
- Connect button
- open system browser
- after callback show "Connected — resuming your task"
- resume automatically.

## Local model UI

V0:
`Needle 2 — Local router — Ready`

Later states:
- not installed
- downloading %
- verifying
- ready
- update available
- failed.

## Developer mode

Provide:
- capability search debugger
- skill search debugger
- model routing debugger
- selected tools sent to model
- installed tool count
- candidate count
- context token estimate
- Needle confidence
- escalation reason
- provider latency
- tool latency.

This is required for routing/eval tuning.

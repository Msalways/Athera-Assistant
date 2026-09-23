# PRV-001 Rig Provider Coverage Manifest

Status: **audited inventory; catalog coverage is incomplete**

Pinned revision: `rig-core 0.42.0` (latest release as of 2026-09-22)
Source: https://github.com/0xPlaygrounds/rig (tag v0.42.0)

This manifest is generated from the Rig source, not marketing lists.
Every provider entry below includes its actual module path, auth expectations,
transport requirements, and Athera adoption status.

## 1. Rig crate structure

| Crate | Purpose | Athera needs |
| --- | --- | --- |
| `rig-core` | Provider-neutral completion, embedding, message, tool contracts; built-in provider wires | Yes (minimal feature set) |
| `rig-agent` | Agent builder, prompt/streaming traits, tool registry, run-loop | No (Athera owns orchestration) |
| `rig-reqwest` | Bundled HTTP transport (reqwest-based) | Yes |
| `rig-derive` | Proc macros (derive, rig_tool) | No (Athera defines its own contracts) |
| `rig-cassette` | Recording/replay for provider tests | Later (PRV-006) |
| `rig-tungstenite` | WebSocket transport backend | Later (if streaming WS needed) |
| `rig-bedrock` | AWS Bedrock native provider | Later (PVD-005) |
| `rig-gemini-grpc` | Google Gemini gRPC | Later (PVD-006) |
| `rig-vertexai` | Google Vertex AI | Later (PVD-007) |
| `rig-candle` | Local model inference | Optional (never required) |

## 2. Provider inventory

### 2.1 Providers in rig-core (always available)

| # | Provider | Module path | Auth expectation | Transport | OpenAI-compatible | Athera priority |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | OpenAI | `providers::openai` | API key (env: OPENAI_API_KEY) | HTTPS/bundled | native | PVD-001 |
| 2 | Anthropic | `providers::anthropic` | API key (env: ANTHROPIC_API_KEY) | HTTPS/bundled | No (native) | PVD-004 |
| 3 | Azure OpenAI | `providers::azure` | API key + endpoint + deployment | HTTPS/bundled | Yes | PVD-008 |
| 4 | Cohere | `providers::cohere` | API key (env: COHERE_API_KEY) | HTTPS/bundled | No | PVD-009 |
| 5 | DeepSeek | `providers::deepseek` | API key | HTTPS/bundled | Yes | PVD-009 |
| 6 | Gemini (REST) | `providers::gemini` | API key (env: GEMINI_API_KEY) | HTTPS/bundled | No | PVD-006 |
| 7 | Groq | `providers::groq` | API key (env: GROQ_API_KEY) | HTTPS/bundled | Yes | PVD-009 |
| 8 | Hugging Face | `providers::huggingface` | API key (env: HUGGINGFACEHUB_API_TOKEN) | HTTPS/bundled | Yes | PVD-009 |
| 9 | Hyperbolic | `providers::hyperbolic` | API key | HTTPS/bundled | Yes | PVD-009 |
| 10 | MiniMax | `providers::minimax` | API key | HTTPS/bundled | Yes | PVD-009 |
| 11 | Mistral | `providers::mistral` | API key (env: MISTRAL_API_KEY) | HTTPS/bundled | Yes | PVD-009 |
| 12 | Moonshot | `providers::moonshot` | API key | HTTPS/bundled | Yes | PVD-009 |
| 13 | Ollama | `providers::ollama` | None (local) | HTTP/localhost | Yes | PVD-009 |
| 14 | OpenRouter | `providers::openrouter` | API key | HTTPS/bundled | Yes | PVD-009 |
| 15 | Perplexity | `providers::perplexity` | API key | HTTPS/bundled | Yes | PVD-009 |
| 16 | Together | `providers::together` | API key (env: TOGETHER_API_KEY) | HTTPS/bundled | Yes | PVD-009 |
| 17 | Venice | `providers::venice` | API key | HTTPS/bundled | Yes | PVD-009 |
| 18 | Voyage AI | `providers::voyageai` | API key | HTTPS/bundled | No (embedding only) | PVD-009 |
| 19 | xAI | `providers::xai` | API key | HTTPS/bundled | Yes | PVD-009 |
| 20 | Xiaomi MiMo | `providers::xiaomimimo` | API key | HTTPS/bundled | Yes | PVD-009 |
| 21 | Z.ai | `providers::zai` | API key | HTTPS/bundled | Yes | PVD-009 |
| 22 | ChatGPT (OAuth) | `providers::chatgpt` | OAuth (browser flow) | HTTPS | Yes | Deferred |
| 23 | Copilot (OAuth) | `providers::copilot` | OAuth (browser flow) | HTTPS | Yes | Deferred |
| 24 | Mira | `providers::mira` | API key | HTTPS/bundled | Yes | PVD-009 |
| 25 | Llamafile | `providers::llamafile` | None (local) | HTTP/localhost | Yes | PVD-009 |
| 26 | Doubleword | `providers::doubleword` | API key | HTTPS/bundled | Yes | PVD-009 |

### 2.2 Providers in companion crates (feature-gated)

| # | Provider | Crate | Feature flag | Auth expectation | Athera priority |
| --- | --- | --- | --- | --- | --- |
| 27 | AWS Bedrock | `rig-bedrock` | `bedrock` | AWS credentials (SigV4) | PVD-005 |
| 28 | Google Gemini gRPC | `rig-gemini-grpc` | `gemini-grpc` | Google Cloud auth | PVD-006 |
| 29 | Google Vertex AI | `rig-vertexai` | `vertexai` | Google Cloud auth | PVD-007 |

## 3. Auth family classification

| Auth family | Providers using it | Athera resolver |
| --- | --- | --- |
| **API key (header)** | OpenAI, Anthropic, Cohere, DeepSeek, Groq, HuggingFace, Hyperbolic, MiniMax, Mistral, Moonshot, OpenRouter, Perplexity, Together, Venice, xAI, XiaomiMiMo, Zai, Mira, Doubleword, AzureOpenAI | AUTH-001 |
| **API key (env var)** | All above (env var names differ per provider) | AUTH-001 |
| **OAuth (browser)** | ChatGPT, Copilot | AUTH-003 |
| **AWS SigV4** | Bedrock | AUTH-004 |
| **Google Cloud auth** | Gemini gRPC, Vertex AI | AUTH-005 |
| **None (local)** | Ollama, Llamafile | N/A |

## 4. Transport requirements

| Transport | Providers | Notes |
| --- | --- | --- |
| HTTPS (reqwest) | All cloud providers | Bundled in `rig-reqwest` |
| HTTP (localhost) | Ollama, Llamafile | Local-only, no TLS |
| gRPC (tonic) | Gemini gRPC | Requires `tonic` dependency |
| WebSocket | (future streaming) | Behind `websocket` feature |

## 5. Feature flags for minimal Android build

```toml
# Minimal: OpenAI-compatible providers only
rig-core = { version = "0.42.0", default-features = false }
rig-reqwest = { version = "0.42.0", default-features = false, features = ["rustls"] }

# Phase 1: OpenAI + Anthropic + DeepSeek + Groq + Mistral + xAI + OpenRouter
# (all built into rig-core, no extra features needed)

# Phase 2: Bedrock
# rig-bedrock = { version = "0.42.0", features = ["bedrock"] }

# Phase 3: Gemini
# rig-gemini-grpc = { version = "0.42.0", features = ["gemini-grpc"] }
```

## 6. Athera adoption plan

### Phase 1 (PRV-002 through PRV-006)
- Provider catalog registry with all 26 rig-core providers
- Connection-test service for API-key providers
- Provider factory mapping profile + auth to model client

### Phase 2 (PVD-001 through PVD-010)
- PVD-001: OpenAI direct (first live provider)
- PVD-002: Generic OpenAI-compatible (covers 15+ providers)
- PVD-003: NVIDIA NIM (via OpenAI-compatible)
- PVD-004: Anthropic direct
- PVD-005: AWS Bedrock
- PVD-006: Gemini
- PVD-007: Vertex AI
- PVD-008: Azure OpenAI
- PVD-009: Remaining rig-core providers (one catalog entry each)
- PVD-010: Remaining companion-crate providers

### Phase 3 (deferred)
- ChatGPT/Copilot OAuth (AUTH-003)
- rig-agent evaluation for subagent inference
- Local model support via rig-candle

## 7. What Rig does NOT own in Athera

Per AGENTS.md and the audit:
- Task orchestration, work graphs, policy, approval, recovery
- Credential storage, OAuth lifecycle, Android Keystore
- Preference learning, user rules
- Capability security, MCP trust boundaries
- Background scheduling, process recovery
- UI rendering, React communication

## 8. Coverage verification

| Check | Status |
| --- | --- |
| Every rig-core provider has a catalog entry | **Incomplete: 21/26**. Missing ChatGPT, Cohere, Copilot, Llamafile and Voyage AI |
| Every provider's auth expectation is classified | **Needs provider-by-provider verification**. The current bulk API-key seed uses one header convention for providers with different wire formats |
| Every provider's transport is classified | Complete (this document) |
| OpenAI-compatible providers identified | 15 of 26 |
| Feature flags documented for minimal build | Complete |
| No marketing-list assumptions | Verified (source-derived) |

## 9. Audit correction (2026-09-23)

The inventory above is derived from the `rig-core 0.42.0` module list, but it is
not yet proof of usable Athera support. The current `provider-rig` crate declares
the Rig dependency without constructing a Rig client or performing inference.
`PRV-001`, `PRV-002` and `PRV-003` therefore remain partial until catalog,
authentication and production-factory coverage are complete.

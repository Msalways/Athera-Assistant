# Web research capability

## Decision

Web research is a first-class graph worker. It uses the same `ToolSpec`, policy,
result-store and event contracts as every other capability. Search vendors, MCP
servers and model-native search remain adapters; none become part of the Rust
orchestration model.

The current runtime can discover and call tools from an unauthenticated Streamable
HTTP MCP server. It does not yet provide a configured search service, MCP OAuth,
structured research events, or automatic source ingestion. The capability must remain
reported as unavailable until a working provider is connected and enabled.

## Provider surface

Normalize provider-specific operations into four semantic operations:

| Operation | Required result |
| --- | --- |
| `web.search` | Ranked URLs with title, snippet, source and optional publication time |
| `web.open` | Extracted page content with canonical URL and retrieval time |
| `web.find` | Matching passages within an already retrieved source |
| `web.capture` | A page or PDF rendering when visual structure is material |

An adapter may implement these with remote MCP tools, provider-native model search,
an enterprise index, or a separately configured search API. The planner normally sees
only the operations needed for the current research step.

Parallel is one verified provider, not a core dependency. Its Search MCP exposes
`web_search` and `web_fetch` over Streamable HTTP. Parallel's native APIs cover the
larger surface: `/v1/search`, `/v1/extract`, Task, FindAll and Monitor. FindAll and
Monitor are not exposed through Parallel MCP, so the native adapter is required when
the graph needs those operations. Search/Extract tuning such as result count and
content budgets stays in the Rust handler rather than being offered to the model as
free-form tool arguments.

MCP tool `outputSchema` and `structuredContent` should be preferred and validated when
present. Unstructured MCP content is accepted only through a bounded normalization
adapter. Raw results are stored separately from model context.

## Research graph

A research request compiles into a bounded graph:

```text
question
  -> clarify scope when necessary
  -> create 1-3 search queries
  -> run searches in parallel
  -> rank and deduplicate candidates
  -> open selected sources
  -> extract relevant passages
  -> verify important claims across sources
  -> synthesize a cited response
  -> render markdown, tables, charts or timelines
```

The default graph limits query count, opened sources, bytes per source, total context,
elapsed time and model calls. Expanding a limit is an orchestrator decision. Pages,
tool descriptions and MCP results are untrusted data and cannot modify graph policy.

For technical research, source selection should prefer primary documentation and
papers. For time-sensitive questions, source records must include retrieval and event
times. A missing or contradictory source must be visible in the result.

## Normalized records

```json
{
  "schema_version": "aethra.web-search-result.v1",
  "query": "example query",
  "results": [
    {
      "title": "Result title",
      "url": "https://example.com/page",
      "snippet": "Bounded provider snippet",
      "source": "configured-provider",
      "published_at": null,
      "rank": 1
    }
  ]
}
```

```json
{
  "schema_version": "aethra.web-source.v1",
  "canonical_url": "https://example.com/page",
  "title": "Page title",
  "retrieved_at": "2026-09-13T12:00:00Z",
  "mime_type": "text/html",
  "content_hash": "sha256:...",
  "passages": [
    {"id": "passage-1", "text": "Bounded extracted text"}
  ]
}
```

Claims in the final answer reference source and passage IDs. The UI link is a locator;
the stored excerpt and hash are the evidence that was actually retrieved.

## Streaming events

Research workers publish the common run-event envelope with these payload types:

- `research_started`;
- `search_started` and `search_completed`;
- `source_selected`, `source_opened` and `source_rejected`;
- `claim_checked`;
- `worker_progress`, `worker_completed` and `worker_failed`;
- `source_added`;
- `block_delta` for narrative text;
- `block_upserted` for source tables, comparisons, charts and timelines;
- `research_completed`, `research_failed` or `research_cancelled`.

The user stream shows concise progress. URLs, excerpts, request sizes, provider IDs,
latency and rejection reasons belong in the expandable research view. Credentials,
authorization headers and full raw pages never enter either stream.

## Presentation

The final `aethra.output.v1` document may contain:

- a Markdown explanation;
- a source table;
- a comparison table;
- a timeline;
- metrics;
- a chart that references validated structured data;
- uncertainty or disagreement notes;
- suggested follow-up searches or workflow actions.

Models do not generate executable HTML, React or chart JavaScript. They select typed
blocks and fields. Rust validates result references and React renders trusted
components.

## Authentication and privacy

Remote MCP research providers use MCP OAuth discovery and authorization when offered.
Mobile authorization uses an external browser/Custom Tab and Android Keystore-backed
token storage. Server workers use a server secret store and a separate connection
identity. API-key-only providers require a secret reference owned by their adapter.
Secrets never enter settings, prompts, SQLite task data or traces.

Search queries can themselves reveal private information. The privacy broker classifies
the proposed query before execution and may redact local entity details, request user
approval, route to a private enterprise/LAN provider, or deny the search. Browsing an
authenticated page requires a separately approved browser capability and is not
implied by `web.open`.

Parallel's public Search MCP, OAuth-enforced Search MCP and authenticated Task MCP are
separate connection presets with different auth behavior. Native Parallel API calls
use an `x-api-key` secret profile; Parallel MCP API-key access uses a Bearer profile.
Task/FindAll/Monitor webhooks terminate on the server worker, where the raw body is
signature-verified and deduplicated before it becomes a graph event. See
`MCP_OAUTH_SECURITY.md` for the transport, OAuth and credential contracts.

## Execution ownership

The cloud reasoner proposes queries, source choices and synthesis. Rust owns budgets,
parallelism, cancellation, source persistence, schema validation and policy. MCP
servers perform retrieval only through leased capabilities. Needle is used only for a
later mobile action such as opening a cited page or sharing an approved report.

## Verification gates

Before enabling web research by default, test:

1. Search, open, find and PDF capture through at least one real provider.
2. OAuth-required, denied, expired and rate-limited connections.
3. Malformed structured results and oversized pages.
4. Prompt injection in result snippets and page content.
5. Conflicting, stale and missing sources.
6. Parallel cancellation and partial worker failure.
7. Claim-to-source references and output-schema validation.
8. Streaming reconnection without duplicated sources or blocks.
9. No credentials or raw private memory in persisted events and diagnostics.
10. Honest unavailable state when no research provider is connected.

## Standards references

- [MCP transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [MCP tools and structured results](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
- [MCP resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
- [Parallel MCP quickstart](https://docs.parallel.ai/integrations/mcp/quickstart)
- [Parallel developer tools](https://docs.parallel.ai/integrations/developer-quickstart)

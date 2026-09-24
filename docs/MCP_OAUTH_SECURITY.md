# MCP connections, authorization and secrets

## Verified scope

AETHRA treats transport, authorization and the upstream service account as three
different concerns. A server URL is never enough information to infer credentials,
and an MCP access token is not interchangeable with a token for Gmail, Slack,
Parallel or another upstream API.

The MCP 2025-11-25 specification defines two standard transports: `stdio` and
Streamable HTTP. Streamable HTTP replaces the older HTTP+SSE transport; legacy SSE
is a compatibility mode rather than a third current standard. Custom transports are
allowed only through an explicit adapter that preserves MCP JSON-RPC and lifecycle
rules.

The Rust adapter implements Streamable HTTP discovery and calls for anonymous,
static Bearer, and reviewed API-key-header profiles. It resolves credentials through
an endpoint-bound interface before opening the transport, disables redirects, maps
MCP 401 challenges to an authentication pause, and keeps credential values out of
the manifest. OAuth manifests can be validated and bound to an existing opaque token
reference; browser authorization, refresh and Android Keystore storage are
implemented behind their native boundary. Stdio, legacy SSE fallback, resources,
prompts and elicitation remain unavailable until their verification gates pass.

## Runtime placement

| Transport | Android app | Desktop/server host | Decision |
| --- | --- | --- | --- |
| Streamable HTTP | Yes | Yes | Primary remote transport; HTTPS only |
| `stdio` subprocess | No | Yes | The host launches an explicitly approved command |
| Legacy HTTP+SSE | Compatibility only | Compatibility only | Try only when the selected connection enables fallback |
| Custom | No generic mode | Adapter-specific | Install and review a transport adapter |

Android cannot usefully launch the Node, Python or native subprocesses expected by
most stdio MCP servers. A local stdio server therefore runs on a paired desktop or
AETHRA server worker and is exposed to the phone through the authenticated AETHRA
gateway. This still gives the product system-wide MCP coverage without shipping an
arbitrary process launcher in the APK.

Transport selection is explicit in stored configuration. `auto` may mean the
spec-defined Streamable HTTP initialization followed by legacy SSE fallback on the
documented HTTP status codes. It never means probing unrelated protocols or sending
credentials to another endpoint.

## Connection manifest

Persist a versioned, non-secret manifest. Secret values are represented only by
opaque references:

```json
{
  "schema_version": "aethra.mcp-connection.v1",
  "id": "parallel-task",
  "name": "Parallel Task",
  "runtime": "remote_http",
  "transport": {
    "kind": "streamable_http",
    "url": "https://task-mcp.parallel.ai/mcp",
    "legacy_sse_fallback": false
  },
  "auth": {
    "kind": "oauth_authorization_code",
    "registration": "discovery",
    "requested_scopes": []
  },
  "enabled": true
}
```

The Rust contracts use tagged enums for `runtime`, `transport` and `auth`; they do
not use a free-form map. An adapter may attach non-secret vendor metadata under its
own versioned namespace. Unknown enum variants, headers, redirect schemes and
transport fields fail closed.

## Authentication profiles

| Profile | Use | Stored configuration | Secret material |
| --- | --- | --- | --- |
| `none` | Public server | Explicit selection | None |
| `oauth_authorization_code` | Interactive user access | Discovery policy, client registration mode, requested scopes | Access/refresh token refs and PKCE transaction state |
| `oauth_client_credentials` | Background server worker | Client ID, token audience/resource and scopes | Client secret or private-key ref |
| `enterprise_managed` | Organization-controlled access | Issuer and negotiated extension metadata | Token refs managed by enterprise policy |
| `bearer_token` | Provider API key or static token in `Authorization` | Header scheme plus secret ref | Token value |
| `api_key_header` | Provider-specific header such as `x-api-key` | Reviewed header name plus secret ref | Key value |
| `basic` | Legacy server explicitly requiring HTTP Basic | Username ref and password ref | Both values |
| `mutual_tls` | Enterprise/private MCP | Certificate and key refs | Private key and certificate chain |
| `signed_request` | Vendor-specific HMAC/JWT/SigV4-style auth | Named auth-adapter ID and non-secret options | Signing-key refs |
| `stdio_environment` | Local subprocess reads environment credentials | Environment-name to secret-ref mapping | Values injected only into that child process |

Only `none`, standard OAuth and a reviewed credential profile belong in the generic
UI. `signed_request` and other proprietary flows require a small adapter with tests;
the generic client does not guess signing algorithms. Basic authentication is
disabled by default and all remote credential profiles require HTTPS. Query-string
credentials and credentials embedded in URLs are rejected.

## Standard OAuth flow

For an HTTP MCP connection selected as OAuth, `AuthManager` owns this deterministic
state machine:

```text
disconnected
  -> discovering protected-resource metadata
  -> selecting advertised authorization server
  -> resolving client registration
  -> waiting_for_user_authorization
  -> exchanging authorization code with PKCE
  -> connected
  -> refreshing | waiting_for_step_up | revoked | failed
```

Implementation requirements:

1. Parse `WWW-Authenticate` and RFC 9728 protected-resource metadata. Do not build
   provider endpoints from names or URL patterns.
2. Discover RFC 8414 and OpenID Connect metadata using the specified lookup order.
3. Resolve the client in this order: configured preregistration, Client ID Metadata
   Document, Dynamic Client Registration, then explicit user/admin configuration.
4. Use Authorization Code with PKCE for the Android public client. Open consent in a
   Custom Tab and return through an app/universal link whose state and redirect URI
   are validated by Kotlin before Rust receives the result.
5. Include the canonical MCP resource URI in authorization and token requests. Bind
   tokens to the connection ID, origin, resource and authorization server.
6. Send bearer tokens in the `Authorization` header on every MCP HTTP request. Never
   put them in a URL.
7. Refresh before expiry, rotate refresh tokens when returned, and replace the old
   token atomically. A 401 pauses the task; it does not cause an unbounded retry.
8. On an `insufficient_scope` challenge, show the additional scopes and request
   step-up authorization. Retry the original call only within the task retry budget.
9. Never pass through a token issued for another resource, provider or MCP server.

OAuth Client Credentials and Enterprise-Managed Authorization are official MCP
extensions. They are enabled only when both sides negotiate the extension. Client
Credentials belongs on the server worker, not in a distributable APK.

## Implemented static-credential boundary

`bearer_token` and `api_key_header` manifests contain only a `secret_ref`. The
runtime's session resolver matches the connection ID, normalized HTTPS origin,
resource, credential purpose and reference before returning a value to the transport.
Editing the endpoint invalidates the binding. The generic API-key profile accepts
only the reviewed `x-api-key` and `api-key` names; Bearer uses the dedicated profile.
Other signing/header schemes require a named adapter. HTTP redirects are disabled so
a credential cannot follow a cross-origin redirect.

The `connect_mcp_with_credential` command accepts a user-entered value, keeps it in
process memory, and saves only the manifest. `save_mcp_credential`,
`clear_mcp_credential`, and `mcp_connection_status` provide repair and redacted status
operations. A process restart removes these host-development credentials. Android
uses Keystore-backed handles for OAuth token records; the callback domain and live
device proof remain required for the OAuth gate.

The host OAuth contract parses Bearer challenges, builds the RFC 9728 protected
resource discovery order and the required RFC 8414/OpenID discovery order, validates
extensible resource/authorization-server metadata, and selects preregistration,
Client ID Metadata Documents, Dynamic Client Registration, or explicit setup in that
order. Dynamic registration requests a public client with no token-endpoint secret.
Its authorization transaction uses 256-bit random state and verifier values,
S256 PKCE, the MCP `resource` parameter, a ten-minute expiry, exact HTTPS callback
binding, duplicate-parameter rejection, and one-time consumption. Public-client code
exchange and refresh requests include the resource, use a no-redirect client, bound
responses to 64 KiB, accept only Bearer tokens, compute bounded expiry, and preserve
or atomically replace a rotated refresh token in the returned token set.

`app-runtime::OAuthRuntime` now owns opaque one-time transaction handles, exact
connection/resource bindings, session-only host tokens, expiry status and refresh.
`start_mcp_oauth`, `complete_mcp_oauth`, and `cancel_mcp_oauth` expose the flow without
returning token values. Completion reconnects the MCP transport with the current
Bearer value and resumes only the task ID bound to that transaction. OAuth transports
are recreated before calls because the upstream HTTP transport owns a fixed header.
The setup UI supports pre-registration, Client ID Metadata Documents and advertised
dynamic registration, with a manual callback field for host testing.

The Android backend now launches the authorization URL in a Custom Tab and accepts
only an exact HTTPS App Link callback (scheme, host, port and path). React receives
neither the authorization URL nor the callback URL on Android. A native AES-GCM
Keystore boundary binds encrypted values to connection metadata as associated data.
The manifest currently uses a placeholder callback host; production domain and
`assetlinks.json` verification are required. OAuthRuntime commits and restores the
bound token record through the Keystore handle, while host tokens intentionally
remain session-only.

Validated `WWW-Authenticate` challenges are retained by connection and surface only
authorization-server and scope metadata through `aethra.connection-state.v1`.
`insufficient_scope` produces a step-up-required state and never exposes a token.
The original task remains subject to its retry budget, and a write that paused after
dispatch is not replayed automatically. Live consent/token/authenticated-call proof
and upstream 403 coverage remain gated work.

## Upstream service authorization

An MCP server may itself need the user to connect an upstream account. MCP 2025-11-25
URL-mode elicitation can send the user to a provider-controlled HTTPS page without
exposing the upstream credential to AETHRA. This is separate from authorizing AETHRA
to call the MCP server.

Form-mode elicitation must never request passwords, API keys, access tokens or other
secrets. URL-mode completion notifications are treated as hints: AETHRA validates the
elicitation ID and then rechecks connection state. The task keeps a manual Retry and
Cancel path if no completion notification arrives.

## Secret ownership

Configuration, SQLite, frontend state and graph events contain `secret_ref` values
only. Secret values live in:

- Android Keystore-backed encrypted storage for user OAuth and user-entered keys;
- the server secret manager for service credentials, webhook secrets and mTLS keys;
- a process environment provider for local development and explicitly mapped stdio
  child environments.

The resolver is scoped by connection ID, normalized origin, resource and credential
purpose. Editing an endpoint invalidates its credential binding. Secrets never enter
model prompts, context packets, normal SQLite columns, logs, traces, diagnostics,
eval fixtures, URL query strings or tool results. The frontend can read only status,
expiry, granted scopes and a masked credential label.

## Task and UI behavior

Authorization is runtime control flow, not model reasoning:

1. Persist the task, graph node and exact tool call.
2. Set `waiting_for_auth` with connection, reason and requested scopes.
3. Complete the selected browser, administrator or credential-entry flow.
4. Reconnect and refresh the tool catalogue.
5. Re-run policy because scopes or tool schemas may have changed.
6. Resume the same graph node once. Surface denial, expiry and uncertainty.

The connection screen asks for runtime, transport and authentication type before
connecting. It shows `discovering`, `auth required`, `authorizing`, `connected`,
`refreshing`, `expired`, `rate limited`, `unavailable` and `failed`. Discovered tools
remain disabled until reviewed. A model cannot create a connection, choose an auth
profile, supply credentials or enable a tool.

## Parallel integration verified on 2026-09-13

Parallel exposes two MCP services using Streamable HTTP:

| Capability | Endpoint | Authentication |
| --- | --- | --- |
| Search and fetch | `https://search.parallel.ai/mcp` | Anonymous by default; optional Parallel API key as Bearer for higher limits; no OAuth discovery |
| Search and fetch, auth enforced | `https://search.parallel.ai/mcp-oauth` | OAuth or Parallel API key as Bearer; anonymous requests return 401 |
| Deep research/task groups | `https://task-mcp.parallel.ai/mcp` | OAuth or Parallel API key as Bearer |

These are three separate connection presets because their authentication behavior is
different. AETHRA must not interpret a successful anonymous Search connection as an
OAuth connection.

Parallel's OAuth Provider uses Authorization Code with mandatory PKCE and no client
secret. Its returned `access_token` is the user's selected Parallel API key. AETHRA
stores it as an opaque endpoint-bound secret even though it is named an access token.

Parallel MCP does not expose FindAll or Monitor. Production web research therefore
gets a separate `provider-parallel` Rust adapter for `/v1/search`, `/v1/extract`, Task,
FindAll and Monitor when those operations are required. Direct Search/Extract use an
`x-api-key` profile; this does not change the MCP connection's Bearer profile.

Task, FindAll and Monitor completions may arrive through server-side webhooks. The
webhook worker verifies the raw body with Standard Webhooks HMAC-SHA256 using
`webhook-id`, `webhook-timestamp` and every `v1` signature, enforces a timestamp
window, deduplicates by webhook ID, acknowledges quickly and processes asynchronously.
The webhook secret remains in the server vault. A phone is not a reliable public
webhook receiver.

## Verification gates

No profile or transport is labelled supported until its row passes protocol,
security and recovery tests:

1. Streamable HTTP JSON and SSE responses, session deletion, reconnect and explicit
   cancellation.
2. stdio framing, child shutdown, stderr bounds, command allowlist and per-child
   environment isolation on desktop/server.
3. Legacy SSE fallback only after the documented Streamable HTTP initialization
   response codes; no silent cross-origin redirect.
4. OAuth protected-resource and authorization-server discovery, every registration
   mode, PKCE callback validation, denial, refresh rotation, revocation and step-up.
5. Static bearer, API-key header, Basic and mTLS origin binding and redaction.
6. Client Credentials and Enterprise-Managed extension negotiation; absence of the
   extension fails closed.
7. URL elicitation completion, cancellation, spoofed IDs and missing notifications;
   reject secrets requested through form mode.
8. Endpoint edits, redirects, DNS changes, certificate failures and attempts to pass
   a token to a different resource.
9. Tool schema/catalogue changes disable affected tools before task resumption.
10. Parallel anonymous Search, OAuth Search, bearer Task and signed webhook fixtures,
    followed by live tests recorded separately.

## References

- [MCP transports, 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [MCP authorization, 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
- [MCP authorization extensions](https://modelcontextprotocol.io/extensions/auth/overview)
- [MCP elicitation, 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/client/elicitation)
- [Parallel developer tools](https://docs.parallel.ai/integrations/developer-quickstart)
- [Parallel MCP quickstart](https://docs.parallel.ai/integrations/mcp/quickstart)
- [Parallel MCP programmatic use](https://docs.parallel.ai/integrations/mcp/programmatic-use)
- [Parallel OAuth Provider](https://docs.parallel.ai/integrations/oauth-provider)
- [Parallel webhook verification](https://docs.parallel.ai/resources/webhook-setup)

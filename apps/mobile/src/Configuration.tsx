import { useEffect, useState } from "react";
import {
  Cable,
  Check,
  Download,
  Info,
  Search,
  Settings2,
  Trash2,
  Unplug,
} from "lucide-react";
import type {
  BuildInfo,
  Capability,
  ConnectionState,
  OAuthAuthorizationStart,
  OAuthAuthorizationPoll,
  ProviderProfileDraft,
  SavedProviderProfile,
  Snapshot,
  Skill,
  Risk,
} from "./types";
import type { ModelStatus, PersonalMemory } from "./types";
import {
  command,
  deleteProviderProfile,
  getBuildInfo,
  listProviderProfiles,
  saveProviderProfile,
  setActiveProvider,
  testProviderConnection,
} from "./service";
import type { ConnectionTestOutcome } from "./service";
import { AdaptiveRulesSettings } from "./AdaptiveRules";
import { ProviderCatalogSettings } from "./ProviderCatalogSettings";

type Act = (name: string, payload: unknown) => Promise<boolean>;
export function Configuration({
  snapshot,
  view,
  act,
}: {
  snapshot: Snapshot;
  view: "connections" | "settings";
  act: Act;
}) {
  const [tab, setTab] = useState("models");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [importText, setImportText] = useState("");
  const [importError, setImportError] = useState("");
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [mcpAuth, setMcpAuth] = useState<
    "none" | "bearer_token" | "api_key_header" | "oauth_authorization_code"
  >("none");
  const [mcpSecret, setMcpSecret] = useState("");
  const [authorizationServer, setAuthorizationServer] = useState("");
  const [oauthScopes, setOauthScopes] = useState("");
  const [oauthClientId, setOauthClientId] = useState("");
  const [clientMetadataUrl, setClientMetadataUrl] = useState("");
  const [redirectUri, setRedirectUri] = useState("");
  const [oauthStart, setOauthStart] = useState<OAuthAuthorizationStart | null>(
    null,
  );
  const [callbackUrl, setCallbackUrl] = useState("");
  const [oauthError, setOauthError] = useState("");
  const [oauthNotice, setOauthNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const run = async (command: string, payload: unknown) => {
    setBusy(true);
    try {
      return await act(command, payload);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    if (!oauthStart || oauthStart.authorization_url) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const status = await command<OAuthAuthorizationPoll>(
          "poll_authorization",
          { transaction_id: oauthStart.transaction_id },
        );
        if (!active) return;
        if (status.state === "connected") {
          setOauthStart(null);
          setOauthNotice("Connected. Resuming your task.");
          return;
        }
        timer = setTimeout(() => void poll(), 750);
      } catch (error) {
        if (!active) return;
        setOauthStart(null);
        setOauthNotice("");
        setOauthError(
          error instanceof Error ? error.message : "Authorization failed",
        );
      }
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [oauthStart]);
  async function addConnection() {
    const id = crypto.randomUUID();
    const authentication =
      mcpAuth === "none"
        ? { kind: "none" as const }
        : mcpAuth === "bearer_token"
          ? {
              kind: "bearer_token" as const,
              secret_ref: `session:mcp:${id}`,
            }
          : mcpAuth === "api_key_header"
            ? {
                kind: "api_key_header" as const,
                header: "x-api-key",
                secret_ref: `session:mcp:${id}`,
              }
            : {
                kind: "oauth_authorization_code" as const,
                token_ref: `keystore:oauth:${id}`,
                authorization_server: authorizationServer,
                resource: url,
                requested_scopes: oauthScopes.split(/\s+/).filter(Boolean),
              };
    const connection = {
      schema: "aethra.mcp-connection.v1" as const,
      id,
      name,
      url,
      transport: "streamable_http" as const,
      authentication,
    };
    if (mcpAuth !== "oauth_authorization_code") {
      await run(
        mcpAuth === "none" ? "connect_mcp" : "connect_mcp_with_credential",
        mcpAuth === "none" ? connection : { connection, secret: mcpSecret },
      );
      return;
    }
    setBusy(true);
    setOauthError("");
    setOauthNotice("");
    try {
      if (!(await act("save_mcp_connection", connection))) return;
      const start = await command<OAuthAuthorizationStart>(
        "authorize_connection",
        {
          connection_id: id,
          client_id: oauthClientId || null,
          client_metadata_url: clientMetadataUrl || null,
          redirect_uri: redirectUri,
          resume_task_id: null,
        },
      );
      setOauthStart(start);
      setOauthNotice(
        start.authorization_url
          ? "Authorization is ready in your browser."
          : "Secure sign-in opened. Return here when finished.",
      );
    } catch (error) {
      setOauthError(
        error instanceof Error
          ? error.message
          : "Could not start authorization",
      );
    } finally {
      setBusy(false);
    }
  }
  async function importSkill() {
    try {
      const skill = JSON.parse(importText) as Skill;
      if (
        !skill.id ||
        !skill.version ||
        !Array.isArray(skill.tool_requirements)
      )
        throw new Error("A skill needs an ID, version and tool requirements.");
      setImportError("");
      await run("save_capability", { kind: "skill", spec: skill });
    } catch (e) {
      setImportError(e instanceof Error ? e.message : "Invalid skill");
    }
  }
  if (view === "connections")
    return (
      <section className="page">
        <h2>Connections</h2>
        {!snapshot.settings.connections.some(
          (connection) => connection.preset === "parallel_search",
        ) && (
          <section className="connection-preset">
            <div>
              <h3>Parallel Search</h3>
              <p>
                Anonymous web search and source retrieval. No account needed.
              </p>
              <small>Tools stay off until you enable them in Settings.</small>
            </div>
            <button
              className="primary"
              disabled={busy}
              onClick={() => void run("connect_parallel_search", {})}
            >
              <Cable size={17} />
              {busy ? "Connecting..." : "Connect Parallel Search"}
            </button>
          </section>
        )}
        {snapshot.settings.connections.length === 0 && (
          <p className="empty-state">No connections yet.</p>
        )}
        {snapshot.settings.connections.map((connection) => (
          <div className="connection-row" key={connection.id}>
            <Cable size={22} />
            <div>
              <h3>{connection.name}</h3>
              <small>{connection.url}</small>
              {(connection.authentication === "none" ||
                connection.authentication.kind === "none") && (
                <span className="badge green">Anonymous</span>
              )}
              {connectionState(snapshot.connections, connection.id) && (
                <>
                  <span
                    className={
                      "badge " +
                      (connectionState(snapshot.connections, connection.id)
                        ?.state === "connected"
                        ? "green"
                        : "amber")
                    }
                  >
                    {connectionStateLabel(
                      connectionState(snapshot.connections, connection.id)!,
                    )}
                  </span>
                  <small>
                    {
                      connectionState(snapshot.connections, connection.id)
                        ?.message
                    }
                  </small>
                </>
              )}
            </div>
            <button
              className="icon-button"
              title="Reconnect and refresh tools"
              aria-label={`Reconnect ${connection.name}`}
              disabled={busy}
              onClick={() => void run("connect_mcp", connection)}
            >
              <Download size={18} />
            </button>
            <button
              className="icon-button"
              title="Disconnect"
              aria-label={`Disconnect ${connection.name}`}
              disabled={busy}
              onClick={() => void run("disconnect_mcp", { id: connection.id })}
            >
              <Unplug size={18} />
            </button>
          </div>
        ))}
        <form
          className="settings-form"
          onSubmit={(e) => {
            e.preventDefault();
            void addConnection();
          }}
        >
          <h3>Add MCP connection</h3>
          <label>
            Name
            <input
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={100}
            />
          </label>
          <label>
            Authentication
            <select
              value={mcpAuth}
              onChange={(event) =>
                setMcpAuth(
                  event.target.value as
                    | "none"
                    | "bearer_token"
                    | "api_key_header"
                    | "oauth_authorization_code",
                )
              }
            >
              <option value="none">None</option>
              <option value="bearer_token">Bearer token</option>
              <option value="api_key_header">API key (x-api-key)</option>
              <option value="oauth_authorization_code">
                OAuth authorization code
              </option>
            </select>
          </label>
          {mcpAuth !== "none" && mcpAuth !== "oauth_authorization_code" && (
            <label>
              Credential
              <input
                required
                type="password"
                value={mcpSecret}
                onChange={(event) => setMcpSecret(event.target.value)}
                autoComplete="off"
                maxLength={8192}
              />
              <small>Held for this session and never saved in settings.</small>
            </label>
          )}
          <label>
            Server URL
            <input
              required
              type="url"
              placeholder="https://"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
            />
          </label>
          {mcpAuth === "oauth_authorization_code" && (
            <>
              <label>
                Authorization server issuer
                <input
                  required
                  type="url"
                  placeholder="https://"
                  value={authorizationServer}
                  onChange={(event) =>
                    setAuthorizationServer(event.target.value)
                  }
                />
              </label>
              <label>
                Requested scopes
                <input
                  value={oauthScopes}
                  onChange={(event) => setOauthScopes(event.target.value)}
                  placeholder="read profile"
                />
                <small>
                  Space-separated scopes; discovery must advertise them.
                </small>
              </label>
              <label>
                Redirect URI
                <input
                  required
                  type="url"
                  value={redirectUri}
                  onChange={(event) => setRedirectUri(event.target.value)}
                  placeholder="https://your-domain.example/oauth/callback"
                />
              </label>
              <details>
                <summary>OAuth client registration</summary>
                <label>
                  Pre-registered client ID
                  <input
                    value={oauthClientId}
                    onChange={(event) => setOauthClientId(event.target.value)}
                  />
                </label>
                <label>
                  Client ID metadata document URL
                  <input
                    type="url"
                    value={clientMetadataUrl}
                    onChange={(event) =>
                      setClientMetadataUrl(event.target.value)
                    }
                  />
                </label>
                <small>
                  Leave both empty to use dynamic registration when advertised.
                </small>
              </details>
            </>
          )}
          {oauthError && (
            <p role="alert" className="inline-error">
              {oauthError}
            </p>
          )}
          {oauthNotice && (
            <p className="field-help" aria-live="polite">
              {oauthNotice}
            </p>
          )}
          {oauthStart && (
            <section className="connection-preset" aria-live="polite">
              <div>
                <h3>Authorization ready</h3>
                <p>Sign in and approve the listed scopes in your browser.</p>
                <small>
                  {oauthStart.requested_scopes.join(", ") ||
                    "No scopes requested"}
                </small>
              </div>
              {oauthStart.authorization_url && (
                <>
                  <a
                    className="primary"
                    href={oauthStart.authorization_url}
                    target="_blank"
                    rel="noreferrer"
                  >
                    Open secure sign-in
                  </a>
                  <label>
                    Callback URL
                    <input
                      type="url"
                      value={callbackUrl}
                      onChange={(event) => setCallbackUrl(event.target.value)}
                      placeholder="Used only if automatic app return is unavailable"
                    />
                  </label>
                  <button
                    type="button"
                    className="secondary"
                    disabled={!callbackUrl || busy}
                    onClick={async () => {
                      if (
                        await run("complete_mcp_oauth", {
                          transaction_id: oauthStart.transaction_id,
                          callback_url: callbackUrl,
                        })
                      ) {
                        setOauthStart(null);
                        setCallbackUrl("");
                        setOauthNotice("Connected.");
                      }
                    }}
                  >
                    Complete authorization
                  </button>
                </>
              )}
              <button
                type="button"
                className="secondary"
                disabled={busy}
                onClick={async () => {
                  if (
                    await run("cancel_authorization", {
                      transaction_id: oauthStart.transaction_id,
                    })
                  ) {
                    setOauthStart(null);
                    setOauthNotice("Authorization cancelled.");
                  }
                }}
              >
                Cancel sign-in
              </button>
            </section>
          )}
          <button className="primary" disabled={busy}>
            <Cable size={17} />
            {busy ? "Connecting..." : "Connect"}
          </button>
        </form>
      </section>
    );
  const entries = snapshot.capabilities.filter(
    (c) =>
      c.kind === (tab === "tools" ? "tool" : "skill") &&
      (c.spec.name + " " + c.spec.description)
        .toLowerCase()
        .includes(query.toLowerCase()) &&
      (filter !== "enabled" || c.spec.enabled),
  );
  return (
    <section className="page">
      <h2>Settings</h2>
      <div className="tabs" role="tablist" aria-label="Settings sections">
        {["models", "memory", "rules", "tools", "skills", "diagnostics"].map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            onClick={() => setTab(t)}
          >
            {t[0].toUpperCase() + t.slice(1)}
          </button>
        ))}
      </div>
      {tab === "models" ? (
        <ModelSettings needle={snapshot.needle} />
      ) : tab === "memory" ? (
        <MemorySettings />
      ) : tab === "rules" ? (
        <AdaptiveRulesSettings
          proposals={snapshot.rule_proposals ?? []}
          rules={snapshot.adaptive_rules ?? []}
        />
      ) : tab === "diagnostics" ? (
        <DiagnosticsTab snapshot={snapshot} />
      ) : (
        <>
          <div className="search-row">
            <Search size={18} />
            <input
              aria-label={`Search ${tab}`}
              placeholder={`Search ${tab}`}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <select
              aria-label="Filter capabilities"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            >
              <option value="all">All</option>
              <option value="enabled">Enabled</option>
            </select>
          </div>
          {entries.length === 0 && (
            <p className="empty-state">No {tab} found.</p>
          )}
          {entries.map((entry) => (
            <CapabilityRow key={entry.spec.id} entry={entry} act={act} />
          ))}
          {tab === "skills" && (
            <div className="settings-form">
              <h3>Import skill</h3>
              <label>
                Skill manifest
                <textarea
                  rows={7}
                  value={importText}
                  onChange={(e) => setImportText(e.target.value)}
                  maxLength={20000}
                />
              </label>
              {importError && <p role="alert">{importError}</p>}
              <button
                className="primary"
                disabled={!importText || busy}
                onClick={() => void importSkill()}
              >
                <Download size={17} />
                Import
              </button>
            </div>
          )}
        </>
      )}
    </section>
  );
}
function CapabilityRow({ entry, act }: { entry: Capability; act: Act }) {
  const [busy, setBusy] = useState(false);
  async function save(next: Capability) {
    setBusy(true);
    try {
      await act("save_capability", next);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="capability-row">
      <div className="capability-heading">
        <h3>{entry.spec.name}</h3>
        <label className="switch">
          <input
            type="checkbox"
            aria-label={`Enable ${entry.spec.name}`}
            disabled={busy}
            checked={entry.spec.enabled}
            onChange={(e) =>
              void save({
                ...entry,
                spec: { ...entry.spec, enabled: e.target.checked },
              } as Capability)
            }
          />
          <span />
        </label>
      </div>
      <p>{entry.spec.description}</p>
      <details>
        <summary>
          <Settings2 size={14} /> Details
        </summary>
        <dl>
          <dt>Identifier</dt>
          <dd>{entry.spec.id}</dd>
          <dt>Version</dt>
          <dd>{entry.spec.version}</dd>
        </dl>
        {entry.kind === "tool" ? (
          <label>
            Permission
            <select
              value={entry.spec.risk}
              disabled={busy}
              onChange={(e) =>
                void save({
                  kind: "tool",
                  spec: { ...entry.spec, risk: e.target.value as Risk },
                })
              }
            >
              <option value="read_only">Read only</option>
              <option value="local_safe_write">Safe local write</option>
              <option value="external_write">External write: approval</option>
              <option value="sensitive">Sensitive: approval</option>
              <option value="destructive">Destructive: approval</option>
            </select>
          </label>
        ) : (
          <>
            <h4>Required tools</h4>
            <ul>
              {entry.spec.tool_requirements.map((id) => (
                <li key={id}>{id}</li>
              ))}
            </ul>
          </>
        )}
      </details>
    </div>
  );
}
function ModelSettings({ needle }: { needle: string }) {
  return (
    <>
      <LocalModelSettings />
      <div className="model-status">
        <div>
          <h3>Needle 2</h3>
          <small>Local tool decisions</small>
        </div>
        <span className="badge amber">
          {needle === "ready" ? "Ready" : "Not linked"}
        </span>
      </div>
      <ProviderProfilesSection />
    </>
  );
}

function ProviderProfilesSection() {
  const [profiles, setProfiles] = useState<SavedProviderProfile[]>([]);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const refresh = async () => {
    try {
      setProfiles(await listProviderProfiles());
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load providers");
    }
  };
  useEffect(() => {
    void refresh();
  }, []);
  const save = async (draft: ProviderProfileDraft) => {
    setBusy(true);
    setStatus("");
    setError("");
    try {
      await saveProviderProfile(draft);
      setStatus("Provider saved.");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save provider");
    } finally {
      setBusy(false);
    }
  };
  const remove = async (provider_id: string) => {
    setBusy(true);
    setError("");
    try {
      await deleteProviderProfile(provider_id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not remove provider");
    } finally {
      setBusy(false);
    }
  };
  const activate = async (provider_id: string) => {
    setBusy(true);
    setError("");
    try {
      await setActiveProvider(provider_id);
      setStatus("Active provider updated.");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not set active provider");
    } finally {
      setBusy(false);
    }
  };
  const [testingId, setTestingId] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<
    Record<string, ConnectionTestOutcome>
  >({});
  const test = async (provider_id: string) => {
    setTestingId(provider_id);
    setError("");
    try {
      const outcome = await testProviderConnection(provider_id);
      setTestResults((prev) => ({ ...prev, [provider_id]: outcome }));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not test connection");
    } finally {
      setTestingId(null);
    }
  };
  return (
    <>
      <h3>Cloud providers</h3>
      <p className="field-help">
        Providers and auth fields come from the Rust catalog. Secrets stay on
        this device and are never shown again.
      </p>
      <ProviderCatalogSettings onSave={(draft) => void save(draft)} />
      {busy && <p className="field-help">Working…</p>}
      {status && (
        <p role="status" className="field-help">
          {status}
        </p>
      )}
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <ul className="provider-list">
        {profiles.map(({ profile, key_configured, active }) => (
          <li key={profile.provider_id}>
            <div>
              <strong>
                {profile.display_name ?? profile.provider_id}
              </strong>{" "}
              <small>{profile.auth_option_id}</small>
            </div>
            <span className="badge amber">
              {key_configured ? "Key stored" : "No key"}
            </span>
            {active && <span className="badge green">Active</span>}
            {!active && (
              <button
                className="secondary"
                type="button"
                disabled={busy}
                onClick={() => void activate(profile.provider_id)}
                aria-label={`Use ${profile.display_name ?? profile.provider_id} for inference`}
              >
                Use for inference
              </button>
            )}
            <button
              className="secondary"
              type="button"
              disabled={busy || testingId === profile.provider_id}
              onClick={() => void test(profile.provider_id)}
              aria-label={`Test ${profile.display_name ?? profile.provider_id} connection`}
            >
              {testingId === profile.provider_id ? "Testing…" : "Test connection"}
            </button>
            {testResults[profile.provider_id] && (
              <p
                role="status"
                className={
                  testResults[profile.provider_id]?.success
                    ? "field-help"
                    : "inline-error"
                }
              >
                {testResults[profile.provider_id]?.message}
              </p>
            )}
            <button
              className="secondary"
              type="button"
              disabled={busy}
              onClick={() => void remove(profile.provider_id)}
              aria-label={`Remove ${profile.display_name ?? profile.provider_id}`}
            >
              <Trash2 size={17} /> Remove
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

function LocalModelSettings() {
  const [status, setStatus] = useState<ModelStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const refresh = async () => {
    try {
      setStatus(await command<ModelStatus>("model_status"));
      setError("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not check local model");
    }
  };
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 1000);
    return () => clearInterval(timer);
  }, []);
  const run = async (name: string) => {
    setBusy(true);
    setError("");
    try {
      await command(name, {});
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Model action failed");
    } finally {
      setBusy(false);
    }
  };
  const manifest = status?.manifest ?? status?.installation?.manifest;
  const installation = status?.installation;
  const downloading =
    installation?.status === "downloading" ||
    installation?.status === "verifying";
  const progress =
    manifest && installation
      ? Math.min(
          100,
          Math.round(
            (installation.downloaded_bytes / manifest.size_bytes) * 100,
          ),
        )
      : 0;
  const installationLabel = installation
    ? {
        missing: "Not downloaded",
        downloading: "Downloading",
        verifying: "Verifying",
        installed: "Installed",
        failed: "Download failed",
        cancelled: "Download cancelled",
      }[installation.status]
    : null;
  return (
    <section className="local-model" aria-labelledby="local-model-title">
      <div className="model-status">
        <div>
          <h3 id="local-model-title">Optional offline conversation model</h3>
          <small aria-live="polite">
            {status
              ? (installationLabel ?? availabilityLabel(status.availability))
              : "Checking availability…"}
          </small>
        </div>
        {status && (
          <span
            className={
              "badge " + (status.availability === "ready" ? "green" : "amber")
            }
          >
            {installationLabel ?? availabilityLabel(status.availability)}
          </span>
        )}
      </div>
      {manifest && (
        <dl className="model-disclosure">
          <dt>Model</dt>
          <dd>{manifest.id}</dd>
          <dt>Download size</dt>
          <dd>{formatBytes(manifest.size_bytes)}</dd>
          <dt>License</dt>
          <dd>{manifest.license}</dd>
          <dt>Context</dt>
          <dd>{manifest.context_tokens.toLocaleString()} tokens</dd>
        </dl>
      )}
      {downloading && (
        <>
          <progress
            max={100}
            value={progress}
            aria-label="Model download progress"
          />
          <small>
            {installation?.status === "verifying"
              ? "Verifying download…"
              : `${formatBytes(installation?.downloaded_bytes ?? 0)} of ${formatBytes(manifest?.size_bytes ?? 0)} downloaded (${progress}%)`}
          </small>
        </>
      )}
      {installation?.error && (
        <p role="alert" className="inline-error">
          {installation.error}
        </p>
      )}
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <div className="button-row model-actions">
        {status?.availability === "missing_model" && !downloading && (
          <button
            className="primary"
            disabled={busy}
            onClick={() => void run("install_model")}
          >
            <Download size={17} />
            {installation?.status === "failed"
              ? "Retry download"
              : "Download model"}
          </button>
        )}
        {downloading && (
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void run("cancel_model_download")}
          >
            Cancel setup
          </button>
        )}
        {status?.availability === "ready" && (
          <>
            <button
              className="secondary"
              disabled={busy}
              onClick={() => void run("unload_model")}
            >
              Unload
            </button>
            <button
              className="danger-button"
              disabled={busy}
              onClick={() =>
                confirmRemove
                  ? void run("remove_model")
                  : setConfirmRemove(true)
              }
            >
              <Trash2 size={17} />
              {confirmRemove ? "Confirm remove" : "Remove"}
            </button>
          </>
        )}
      </div>
      {status?.availability === "unavailable" && (
        <p className="field-help">
          Local conversation cannot run on this device. Action mode remains
          available when configured.
        </p>
      )}
      {error && (
        <p className="field-help">
          Local model status is unavailable. Check that the assistant is
          running, then retry from the main screen.
        </p>
      )}
    </section>
  );
}

function MemorySettings() {
  const [memories, setMemories] = useState<PersonalMemory[]>([]);
  const [text, setText] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [error, setError] = useState("");
  const load = async () => {
    try {
      setMemories(await command<PersonalMemory[]>("list_memories"));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load memory");
    }
  };
  useEffect(() => {
    void load();
  }, []);
  async function save() {
    const value = text.trim();
    if (!value) return;
    try {
      await command<PersonalMemory>("save_memory", {
        ...(editingId ? { id: editingId } : {}),
        text: value,
        confirmed: true,
      });
      setText("");
      setEditingId(null);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save memory");
    }
  }
  async function remove(id: string) {
    try {
      await command("delete_memory", { id });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not delete memory");
    }
  }
  return (
    <section className="memory-settings">
      <p className="field-help">
        The assistant uses only memories you explicitly save here.
      </p>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <form
        className="settings-form"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <label htmlFor="memory-text">
          {editingId ? "Edit memory" : "Remember this"}
        </label>
        <textarea
          id="memory-text"
          rows={3}
          value={text}
          maxLength={1000}
          onChange={(e) => setText(e.target.value)}
        />
        <button className="primary" disabled={!text.trim()}>
          <Check size={17} /> Confirm and save
        </button>
        {editingId && (
          <button
            className="secondary"
            type="button"
            onClick={() => {
              setEditingId(null);
              setText("");
            }}
          >
            Cancel edit
          </button>
        )}
      </form>
      {memories.length === 0 ? (
        <p className="empty-state">No saved memories.</p>
      ) : (
        memories.map((memory) => (
          <div className="memory-row" key={memory.id}>
            <p>{memory.text}</p>
            <button
              className="secondary"
              onClick={() => {
                setEditingId(memory.id);
                setText(memory.text);
              }}
            >
              Edit
            </button>
            <button
              className="icon-button"
              aria-label={"Delete memory: " + memory.text}
              onClick={() => void remove(memory.id)}
            >
              <Trash2 size={18} />
            </button>
          </div>
        ))
      )}
    </section>
  );
}

function DiagnosticsTab({ snapshot }: { snapshot: Snapshot }) {
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);
  useEffect(() => {
    void getBuildInfo().then(setBuildInfo);
  }, []);
  return (
    <section className="settings-form">
      <h3>
        <Info size={16} style={{ verticalAlign: "middle", marginRight: 6 }} />
        Developer Diagnostics
      </h3>
      <dl>
        <dt>Version</dt>
        <dd>{buildInfo?.version ?? "unknown"}</dd>
        <dt>Git commit</dt>
        <dd>
          {buildInfo?.git_hash ?? "unknown"}
          {buildInfo?.git_dirty ? " (dirty)" : ""}
        </dd>
        <dt>Build time</dt>
        <dd>
          {buildInfo
            ? new Date(buildInfo.build_timestamp * 1000).toLocaleString()
            : "unknown"}
        </dd>
        <dt>Needle</dt>
        <dd>{snapshot.needle}</dd>
        <dt>Cloud credential</dt>
        <dd>{snapshot.cloud_credential}</dd>
        <dt>Voice</dt>
        <dd>{snapshot.voice}</dd>
        <dt>Capabilities</dt>
        <dd>{snapshot.capabilities.length}</dd>
        <dt>Tasks</dt>
        <dd>{snapshot.tasks.length}</dd>
      </dl>
    </section>
  );
}

function availabilityLabel(value: ModelStatus["availability"]) {
  return {
    missing_model: "Model not downloaded",
    ready: "Ready",
    busy: "Busy",
    unavailable: "Unavailable",
  }[value];
}
function formatBytes(bytes: number) {
  if (bytes < 1024 * 1024) return Math.round(bytes / 1024) + " KB";
  if (bytes < 1024 * 1024 * 1024)
    return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return (bytes / 1024 / 1024 / 1024).toFixed(1) + " GB";
}

function connectionState(states: ConnectionState[], id: string) {
  return states.find((state) => state.connection_id === id);
}

function connectionStateLabel(connection: ConnectionState) {
  return {
    connected: "Connected",
    required: "Authorization required",
    connecting: "Connecting",
    refreshing: "Refreshing",
    expired: "Expired",
    step_up_required: "More access required",
    denied: "Denied",
    revoked: "Revoked",
    unavailable: "Unavailable",
  }[connection.state];
}

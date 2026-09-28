import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  ArrowUp,
  Cable,
  MessageSquare,
  RefreshCw,
  Settings as SettingsIcon,
  X,
} from "lucide-react";
import { command } from "./service";
import type { RunEventEnvelope, RunEventPage, Snapshot, Task } from "./types";
import { TaskView } from "./TaskView";
import { Configuration } from "./Configuration";

type View = "assistant" | "activity" | "connections" | "settings";
type BridgeState = "connecting" | "online" | "degraded" | "offline";
type SendState = "idle" | "sending" | "accepted";

const SNAPSHOT_TIMEOUT_MS = 8000;
const COLD_START_TIMEOUT_MS = 25000;
const COLD_START_ATTEMPTS = 3;

function errorText(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

function getOrCreateConversationId(): string {
  const key = "athera.active_conversation_id";
  try {
    const stored = window.localStorage.getItem(key);
    if (stored) return stored;
    const created = crypto.randomUUID();
    window.localStorage.setItem(key, created);
    return created;
  } catch {
    return crypto.randomUUID();
  }
}

function getStoredDraft(): string {
  try {
    return window.localStorage.getItem("athera.composer_draft") ?? "";
  } catch {
    return "";
  }
}
export default function App() {
  const [view, setView] = useState<View>("assistant");
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const snapshotRef = useRef<Snapshot | null>(null);
  const refreshInFlight = useRef(false);
  const eventPollInFlight = useRef(false);
  const consecutiveFailures = useRef(0);
  const [bridgeState, setBridgeState] = useState<BridgeState>("connecting");
  const [error, setError] = useState("");
  const [eventError, setEventError] = useState("");
  const [actionDraft, setActionDraft] = useState(getStoredDraft);
  const [sendingAction, setSendingAction] = useState(false);
  const [sendState, setSendState] = useState<SendState>("idle");
  const [conversation, setConversation] = useState(() =>
    getOrCreateConversationId(),
  );
  const [selected, setSelected] = useState<string | null>(null);
  const [runEvents, setRunEvents] = useState<RunEventEnvelope[]>([]);
  useEffect(() => {
    try {
      if (actionDraft) {
        window.localStorage.setItem("athera.composer_draft", actionDraft);
      } else {
        window.localStorage.removeItem("athera.composer_draft");
      }
    } catch {
      return;
    }
  }, [actionDraft]);
  const refresh = useCallback(async (showError = true) => {
    if (refreshInFlight.current) return false;
    refreshInFlight.current = true;
    const coldStart = snapshotRef.current === null;
    try {
      const next = await command<Snapshot>(
        "snapshot",
        {},
        { timeoutMs: coldStart ? COLD_START_TIMEOUT_MS : SNAPSHOT_TIMEOUT_MS },
      );
      snapshotRef.current = next;
      setSnapshot(next);
      setBridgeState("online");
      consecutiveFailures.current = 0;
      if (showError) setError("");
      return true;
    } catch (e) {
      consecutiveFailures.current += 1;
      if (coldStart && consecutiveFailures.current < COLD_START_ATTEMPTS) {
        setBridgeState("connecting");
        return false;
      }
      setBridgeState(snapshotRef.current ? "degraded" : "offline");
      if (showError) {
        setError(
          errorText(
            e,
            "Cannot reach the assistant. Check that it is running, then retry.",
          ),
        );
      }
      return false;
    } finally {
      refreshInFlight.current = false;
    }
  }, []);
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 1500);
    return () => clearInterval(timer);
  }, [refresh]);
  useEffect(() => {
    let after = 0;
    const poll = async () => {
      if (eventPollInFlight.current) return;
      eventPollInFlight.current = true;
      try {
        const page = await command<RunEventPage>(
          "run_events",
          { after },
          { timeoutMs: 5000 },
        );
        if (
          page.schema !== "aethra.run-events.v1" ||
          !Array.isArray(page.events) ||
          !Number.isSafeInteger(page.next_after)
        )
          throw new Error("The assistant returned an invalid event update.");
        after = page.next_after;
        setRunEvents((current) => {
          const retained = page.reset_required ? [] : current;
          const unique = new Map(
            retained.map((event) => [event.event_id, event]),
          );
          for (const event of page.events) unique.set(event.event_id, event);
          return [...unique.values()]
            .sort((left, right) => left.sequence - right.sequence)
            .slice(-500);
        });
        setEventError("");
      } catch (e) {
        setEventError(errorText(e, "Live updates paused. Retry to reconnect."));
      } finally {
        eventPollInFlight.current = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 750);
    return () => clearInterval(timer);
  }, []);
  async function act(name: string, payload: unknown) {
    setError("");
    try {
      await command(name, payload, { timeoutMs: 15000 });
      await refresh();
      return true;
    } catch (e) {
      setError(errorText(e, "Action failed"));
      return false;
    }
  }
  async function sendAction() {
    if (!actionDraft.trim() || sendingAction) return;
    const text = actionDraft.trim();
    setSendingAction(true);
    setSendState("sending");
    setError("");
    try {
      await command(
        "submit_input",
        {
          conversation_id: conversation,
          text,
          source: "text",
        },
        { timeoutMs: 10000 },
      );
      setActionDraft("");
      setSendState("accepted");
      const refreshed = await refresh(false);
      if (!refreshed) {
        setError(
          "Your request was accepted, but live status is unavailable. Retry to reconnect.",
        );
      }
    } catch (e) {
      setSendState("idle");
      setError(errorText(e, "Could not send request"));
    } finally {
      setSendingAction(false);
    }
  }
  function startNewChat() {
    const next = crypto.randomUUID();
    try {
      window.localStorage.setItem("athera.active_conversation_id", next);
    } catch {
      setConversation(next);
      setActionDraft("");
      setSendState("idle");
      setSelected(null);
      return;
    }
    setConversation(next);
    setActionDraft("");
    setSendState("idle");
    setSelected(null);
  }
  const tasks = useMemo(
    () =>
      snapshot?.tasks
        .filter((t) => t.input.conversation_id === conversation)
        .slice()
        .reverse() ?? [],
    [conversation, snapshot],
  );
  // Display names for the configured chain, so a turn can be attributed to a
  // vendor the user recognises rather than a slug.
  const providerLabels = useMemo(
    () =>
      Object.fromEntries(
        (snapshot?.failover?.chain ?? []).map((member) => [
          member.provider_id,
          member.display_name,
        ]),
      ),
    [snapshot],
  );
  // The provider a turn should have used. Anything else answering means a
  // fallback happened and must be visible.
  const primaryProviderId = snapshot?.failover?.chain?.[0]?.provider_id ?? null;
  const active = tasks.some(
    (t) => t.status === "running" || t.status === "created",
  );
  const setupRequired =
    snapshot?.cloud_credential === "missing" ||
    snapshot?.cloud_credential === "not_configured";
  const connectionLabel =
    bridgeState === "connecting"
      ? "Starting"
      : bridgeState === "offline"
        ? "Offline"
        : bridgeState === "degraded"
          ? "Connection degraded"
          : active
            ? "Working"
            : setupRequired
              ? "Setup required"
              : "Ready";
  const selection = snapshot?.tasks.find((t) => t.id === selected);
  useEffect(() => {
    if (
      sendState === "accepted" &&
      tasks.some((task) =>
        ["completed", "failed", "cancelled", "waiting_for_auth"].includes(
          task.status,
        ),
      )
    ) {
      setSendState("idle");
    }
  }, [sendState, tasks]);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const newestTaskId = tasks[tasks.length - 1]?.id ?? null;
  useEffect(() => {
    if (view !== "assistant" || !newestTaskId) return;
    const node = scrollerRef.current;
    if (!node) return;
    node.scrollTo({ top: node.scrollHeight, behavior: "auto" });
  }, [view, newestTaskId]);
  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="app-mark">
          <img src="/athera-orbit.png" alt="" />
        </div>
        <div>
          <h1>Athera</h1>
          <span className="connection-state" aria-live="polite">
            <i
              className={
                setupRequired
                  ? "degraded"
                  : bridgeState === "online"
                    ? "online"
                    : bridgeState === "degraded"
                      ? "degraded"
                      : ""
              }
            />
            {connectionLabel}
          </span>
        </div>
        <div className="header-actions">
          {tasks.length > 0 && (
            <button className="text-button new-chat" onClick={startNewChat}>
              New chat
            </button>
          )}
          <button
            className="icon-button"
            title="Refresh"
            aria-label="Refresh"
            onClick={() => {
              setError("");
              void refresh();
            }}
          >
            <RefreshCw size={19} />
          </button>
        </div>
      </header>
      {error && (
        <div role="alert" className="error-banner">
          <span>{error}</span>
          <button
            className="icon-button"
            aria-label="Dismiss error"
            onClick={() => setError("")}
          >
            <X size={18} />
          </button>
        </div>
      )}
      {eventError && !error && (
        <div role="status" className="status-banner">
          <span>{eventError}</span>
          <button className="text-button" onClick={() => void refresh()}>
            Retry
          </button>
        </div>
      )}
      <main ref={scrollerRef}>
        {view === "assistant" && (
          <section className="conversation">
            {tasks.length === 0 ? (
              bridgeState === "connecting" ? (
                <div className="empty-conversation" aria-live="polite">
                  <RefreshCw className="empty-mark" size={28} />
                  <h2>Starting Athera</h2>
                  <p>
                    Loading the assistant, your provider setup, and saved
                    conversations. This can take a moment on first launch.
                  </p>
                </div>
              ) : bridgeState === "offline" ? (
                <div className="empty-conversation">
                  <X className="empty-mark" size={28} />
                  <h2>Assistant unavailable</h2>
                  <p>Reconnect to the assistant before sending a request.</p>
                  <button
                    className="primary"
                    onClick={() => void refresh()}
                    type="button"
                  >
                    Try again
                  </button>
                </div>
              ) : setupRequired ? (
                <div className="empty-conversation">
                  <SettingsIcon className="empty-mark" size={28} />
                  <h2>Connect a provider to start</h2>
                  <p>
                    Athera needs one working cloud provider before it can answer
                    conversation requests.
                  </p>
                  <button
                    className="primary"
                    onClick={() => setView("settings")}
                    type="button"
                  >
                    Configure provider
                  </button>
                </div>
              ) : (
                <div className="empty-conversation">
                  <MessageSquare size={34} />
                  <h2>What would you like me to do?</h2>
                  <p>
                    Athera interprets your request, uses relevant context, and
                    acts within explicit authority.
                  </p>
                </div>
              )
            ) : (
              tasks.map((task) => (
                <TaskView
                  key={task.id}
                  task={task}
                  events={runEvents.filter((event) => event.run_id === task.id)}
                  act={act}
                  onOpenSettings={() => setView("settings")}
                  providerLabels={providerLabels}
                  fallbackProviderId={primaryProviderId}
                />
              ))
            )}
          </section>
        )}
        {view === "activity" && (
          <section className="page">
            <h2>Activity</h2>
            {snapshot?.suggestions && snapshot.suggestions.length > 0 && (
              <section
                aria-label="Suggested next steps"
                className="notice-card"
              >
                <h3>Suggested next steps</h3>
                {snapshot.suggestions.map((suggestion) => (
                  <button
                    className="activity-row"
                    key={`${suggestion.id}-${suggestion.task_id}`}
                    onClick={() => setSelected(suggestion.task_id)}
                  >
                    <span>{suggestion.title}</span>
                    <small>{suggestion.reason}</small>
                  </button>
                ))}
              </section>
            )}
            {selection ? (
              <>
                <button
                  className="text-button"
                  onClick={() => setSelected(null)}
                >
                  Back to activity
                </button>
                <TaskView
                  task={selection}
                  events={runEvents.filter(
                    (event) => event.run_id === selection.id,
                  )}
                  act={act}
                  onOpenSettings={() => setView("settings")}
                  providerLabels={providerLabels}
                  fallbackProviderId={primaryProviderId}
                />
                <details>
                  <summary>Task details</summary>
                  <dl>
                    <dt>Steps</dt>
                    <dd>{selection.step}</dd>
                    <dt>Current role</dt>
                    <dd>{selection.role}</dd>
                    <dt>Stored results</dt>
                    <dd>{selection.result_refs.length}</dd>
                  </dl>
                  {selection.plan.length > 0 && (
                    <ol>
                      {selection.plan.map((s, i) => (
                        <li key={i}>{s}</li>
                      ))}
                    </ol>
                  )}
                </details>
              </>
            ) : snapshot?.tasks.length ? (
              snapshot.tasks.map((task) => (
                <button
                  className="activity-row"
                  key={task.id}
                  onClick={() => setSelected(task.id)}
                >
                  <span>{task.input.text}</span>
                  <small>
                    {label(task.status)} &middot; {task.step}{" "}
                    {task.step === 1 ? "step" : "steps"}
                  </small>
                </button>
              ))
            ) : (
              <p className="empty-state">No tasks yet.</p>
            )}
          </section>
        )}
        {(view === "connections" || view === "settings") && snapshot && (
          <Configuration snapshot={snapshot} view={view} act={act} />
        )}
        {(view === "connections" || view === "settings") && !snapshot && (
          <p className="empty-state">Assistant unavailable</p>
        )}
      </main>
      {view === "assistant" && (
        <form
          className="composer"
          onSubmit={(e) => {
            e.preventDefault();
            void sendAction();
          }}
        >
          <div className="compose-row">
            <label className="sr-only" htmlFor="action-message">
              Ask Athera
            </label>
            <textarea
              id="action-message"
              rows={2}
              placeholder="What would you like me to do?"
              value={actionDraft}
              maxLength={8000}
              onChange={(e) => setActionDraft(e.target.value)}
            />
            <button
              className="send-button"
              aria-label="Send"
              title="Send"
              aria-describedby="composer-hint"
              disabled={
                !actionDraft.trim() ||
                sendingAction ||
                bridgeState !== "online" ||
                setupRequired
              }
              type="submit"
            >
              <ArrowUp size={22} />
            </button>
          </div>
          <p id="composer-hint" className="composer-hint" aria-live="polite">
            {sendState === "sending"
              ? "Sending your request…"
              : sendState === "accepted"
                ? "Request accepted. Waiting for the assistant…"
                : setupRequired
                  ? "Connect a provider to enable conversation."
                  : bridgeState === "connecting"
                    ? "Athera is still starting. You can send once it is ready."
                    : bridgeState !== "online"
                      ? "Waiting for the assistant to reconnect."
                      : "Athera will show progress and any required action here."}
          </p>
        </form>
      )}
      <nav className="bottom-nav" aria-label="Main navigation">
        {(
          [
            { id: "assistant", label: "Athera", Icon: MessageSquare },
            { id: "activity", label: "Activity", Icon: Activity },
            { id: "connections", label: "Connections", Icon: Cable },
            { id: "settings", label: "Settings", Icon: SettingsIcon },
          ] as const
        ).map(({ id, label: caption, Icon }) => (
          <button
            key={id}
            onClick={() => setView(id)}
            aria-current={view === id ? "page" : undefined}
          >
            <Icon size={21} />
            <span>{caption}</span>
          </button>
        ))}
      </nav>
    </div>
  );
}
export function label(status: Task["status"]) {
  return {
    created: "Queued",
    running: "Working",
    waiting_for_auth: "Connection required",
    waiting_for_approval: "Approval required",
    waiting_for_user: "Your response needed",
    waiting_for_resolution: "Check action outcome",
    completed: "Completed",
    failed: "Failed",
    cancelled: "Cancelled",
  }[status];
}

import { useCallback, useEffect, useState } from "react";
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
import type {
  ModelStatus,
  RunEventEnvelope,
  RunEventPage,
  Snapshot,
  Task,
} from "./types";
import { TaskView } from "./TaskView";
import { Configuration } from "./Configuration";
import { ConversationView } from "./ConversationView";

type View = "assistant" | "activity" | "connections" | "settings";
export default function App() {
  const [view, setView] = useState<View>("assistant");
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [model, setModel] = useState<ModelStatus | null>(null);
  const [modelOffline, setModelOffline] = useState(false);
  const [error, setError] = useState("");
  const [actionDraft, setActionDraft] = useState("");
  const [sendingAction, setSendingAction] = useState(false);
  const [actionMode, setActionMode] = useState(false);
  const [conversation] = useState(() => crypto.randomUUID());
  const [selected, setSelected] = useState<string | null>(null);
  const [runEvents, setRunEvents] = useState<RunEventEnvelope[]>([]);
  const refresh = useCallback(async () => {
    try {
      setSnapshot(await command<Snapshot>("snapshot"));
    } catch {
      setError(
        "Cannot reach the assistant. Check that it is running, then retry.",
      );
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
      try {
        const page = await command<RunEventPage>("run_events", { after });
        if (
          page.schema !== "aethra.run-events.v1" ||
          !Array.isArray(page.events) ||
          !Number.isSafeInteger(page.next_after)
        )
          return;
        after = page.next_after;
        setRunEvents((current) => {
          const retained = page.reset_required ? [] : current;
          const known = new Set(retained.map((event) => event.event_id));
          return [
            ...retained,
            ...page.events.filter((event) => !known.has(event.event_id)),
          ].slice(-500);
        });
      } catch {
        // Snapshot polling above owns the user-visible bridge error.
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 750);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    const checkModel = async () => {
      try {
        setModel(await command<ModelStatus>("model_status"));
        setModelOffline(false);
      } catch {
        setModel(null);
        setModelOffline(true);
      }
    };
    void checkModel();
    const timer = setInterval(() => void checkModel(), 1500);
    return () => clearInterval(timer);
  }, []);
  async function act(name: string, payload: unknown) {
    setError("");
    try {
      await command(name, payload);
      await refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Action failed");
      return false;
    }
  }
  async function sendAction() {
    if (!actionDraft.trim() || sendingAction) return;
    setSendingAction(true);
    setError("");
    try {
      await command("submit_input", {
        conversation_id: conversation,
        text: actionDraft,
        source: "text",
      });
      setActionDraft("");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not send request");
    } finally {
      setSendingAction(false);
    }
  }
  const tasks =
    snapshot?.tasks
      .filter((t) => t.input.conversation_id === conversation)
      .slice()
      .reverse() ?? [];
  const active = snapshot?.tasks.some((t) => t.status === "running");
  const selection = snapshot?.tasks.find((t) => t.id === selected);
  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="app-mark">
          <MessageSquare size={22} />
        </div>
        <div>
          <h1>{actionMode ? "Actions" : "Assistant"}</h1>
          <span className="connection-state">
            <i className={snapshot ? "online" : ""} />
            {actionMode
              ? active
                ? "Working"
                : snapshot?.cloud_credential === "missing"
                  ? "API key needed for actions"
                  : "Action mode"
              : modelStatusLabel(model, modelOffline)}
          </span>
        </div>
        <div className="header-actions">
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
      <main>
        {view === "assistant" && (
          <>
            <div
              className="mode-switcher"
              role="group"
              aria-label="Assistant mode"
            >
              <button
                aria-pressed={!actionMode}
                onClick={() => setActionMode(false)}
              >
                Conversation
              </button>
              <button
                aria-pressed={actionMode}
                onClick={() => setActionMode(true)}
              >
                Action
              </button>
            </div>
            {actionMode ? (
              <section className="conversation">
                {tasks.length === 0 ? (
                  <div className="empty-conversation">
                    <MessageSquare size={34} />
                    <h2>What would you like me to do?</h2>
                    <p>
                      Actions can use connected tools and may ask for approval.
                    </p>
                  </div>
                ) : (
                  tasks.map((task) => (
                    <TaskView
                      key={task.id}
                      task={task}
                      events={runEvents.filter(
                        (event) => event.run_id === task.id,
                      )}
                      act={act}
                    />
                  ))
                )}
              </section>
            ) : (
              <ConversationView
                model={model}
                modelOffline={modelOffline}
                onOpenSettings={() => setView("settings")}
              />
            )}
          </>
        )}
        {view === "activity" && (
          <section className="page">
            <h2>Activity</h2>
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
                    {label(task.status)} &middot; {task.step} steps
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
      {view === "assistant" && actionMode && (
        <form
          className="composer"
          onSubmit={(e) => {
            e.preventDefault();
            void sendAction();
          }}
        >
          <div className="compose-row">
            <label className="sr-only" htmlFor="action-message">
              Action request
            </label>
            <textarea
              id="action-message"
              rows={2}
              placeholder="Describe an action…"
              value={actionDraft}
              maxLength={8000}
              onChange={(e) => setActionDraft(e.target.value)}
            />
            <button
              className="send-button"
              aria-label="Send action"
              title="Send action"
              disabled={!actionDraft.trim() || sendingAction || !snapshot}
              type="submit"
            >
              <ArrowUp size={22} />
            </button>
          </div>
        </form>
      )}
      <nav className="bottom-nav" aria-label="Main navigation">
        {(
          [
            { id: "assistant", label: "Assistant", Icon: MessageSquare },
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
function modelStatusLabel(model: ModelStatus | null, offline: boolean) {
  if (!model)
    return offline ? "Local model status unavailable" : "Checking local model";
  if (model.installation?.status === "downloading")
    return "Downloading local model";
  if (model.installation?.status === "verifying")
    return "Verifying local model";
  if (model.installation?.status === "failed")
    return "Local model download failed";
  return {
    missing_model: "Local model needed",
    ready: "Local model ready",
    busy: "Local model busy",
    unavailable: "Local model unavailable",
  }[model.availability];
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

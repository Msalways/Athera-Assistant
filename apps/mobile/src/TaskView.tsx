import { useState } from "react";
import { Check, CircleAlert, RefreshCw, Square, X } from "lucide-react";
import type {
  AssistantOutput,
  RunEventEnvelope,
  RunEventKind,
  Task,
} from "./types";
import { label } from "./App";
import { Markdown } from "./Markdown";
import { PreferenceFeedback } from "./PreferenceFeedback";
export function TaskView({
  task,
  events = [],
  act,
  onOpenSettings,
  providerLabels,
  fallbackProviderId,
}: {
  task: Task;
  events?: RunEventEnvelope[];
  act: (name: string, payload: unknown) => Promise<unknown>;
  onOpenSettings?: () => void;
  providerLabels?: Record<string, string>;
  fallbackProviderId?: string | null;
}) {
  const [answer, setAnswer] = useState("");
  const [busy, setBusy] = useState(false);
  async function perform(name: string, payload: unknown) {
    setBusy(true);
    try {
      await act(name, payload);
    } finally {
      setBusy(false);
    }
  }
  const pending = task.pending;
  const active = task.status === "running" || task.status === "created";
  const streamedText = events
    .filter((event) => event.kind === "text_delta")
    .map((event) => event.text_delta ?? "")
    .join("");
  const workers = workerActivity(events);
  const showMessage =
    task.message.trim().length > 0 &&
    task.message.trim().toLowerCase() !== label(task.status).toLowerCase();
  // Only shown when a fallback chain is active, so an ordinary single-provider
  // reply is not decorated. When the answer came from the fallback, it says so.
  const answeredBy = task.answered_by ?? null;
  const usedFallback =
    Boolean(answeredBy) &&
    Boolean(fallbackProviderId) &&
    answeredBy !== fallbackProviderId;
  const answeredByName = answeredBy
    ? (providerLabels?.[answeredBy] ?? answeredBy)
    : null;
  return (
    <article className="task">
      <div className="user-message">{task.input.text}</div>
      <div className="task-state" aria-live="polite">
        <span className={task.status === "completed" ? "success" : ""}>
          {task.status === "completed" ? (
            <Check size={15} />
          ) : task.status === "failed" ||
            task.status === "waiting_for_resolution" ? (
            <CircleAlert size={15} />
          ) : (
            <span className={active ? "status-dot pulsing" : "status-dot"} />
          )}{" "}
          {label(task.status)}
        </span>
        {answeredByName && (
          <span
            className={`provider-badge${usedFallback ? " fallback" : ""}`}
            title={
              usedFallback
                ? `The primary provider could not answer, so ${answeredByName} did.`
                : `Answered by ${answeredByName}.`
            }
          >
            {usedFallback ? <RefreshCw size={12} aria-hidden /> : null}
            {usedFallback ? "Fallback · " : ""}
            {answeredByName}
          </span>
        )}
        {active && (
          <button
            className="icon-button"
            title="Cancel task"
            aria-label="Cancel task"
            onClick={() => void perform("cancel_task", { task_id: task.id })}
          >
            <Square size={15} />
          </button>
        )}
      </div>
      {task.output ? (
        <OutputView output={task.output} />
      ) : streamedText ? (
        <p className="assistant-message" aria-live="polite">
          {streamedText}
        </p>
      ) : (
        showMessage && <p className="assistant-message">{task.message}</p>
      )}
      {task.status === "completed" && (
        <PreferenceFeedback
          message={{
            id: task.id,
            conversation_id: task.input.conversation_id,
            role: "assistant",
            content: task.message,
            status: "complete",
            created_at: 0,
          }}
        />
      )}
      {events.length > 0 && (
        <details className="execution-events">
          <summary>
            Execution · {events.length}{" "}
            {events.length === 1 ? "event" : "events"}
            {workers.length > 0
              ? ` · ${workers.length} ${workers.length === 1 ? "worker" : "workers"}`
              : ""}
          </summary>
          {workers.length > 0 && (
            <ul className="worker-activity" aria-label="Worker activity">
              {workers.map((worker, index) => (
                <li key={worker.id}>
                  <span>Worker {index + 1}</span>
                  <small>{worker.status}</small>
                </li>
              ))}
            </ul>
          )}
          <ol>
            {events.map((event) => (
              <li key={event.event_id}>
                <span>{eventLabel(event.kind)}</span>
                <small>
                  Step {event.step}
                  {event.worker_id ? " · Worker" : ""}
                </small>
              </li>
            ))}
          </ol>
        </details>
      )}
      {task.status === "waiting_for_approval" && pending && (
        <section className="approval" aria-label="Review action">
          <h3>{pending.spec.name}</h3>
          <p className="approval-summary">
            Review what this action will change before you continue.
          </p>
          <details>
            <summary>Technical details</summary>
            <dl>
              {Object.entries(pending.call.arguments).map(([key, value]) => (
                <div key={key}>
                  <dt>{key}</dt>
                  <dd>
                    {typeof value === "string"
                      ? value
                      : JSON.stringify(value, null, 2)}
                  </dd>
                </div>
              ))}
            </dl>
          </details>
          <div className="button-row">
            <button
              className="secondary"
              disabled={busy}
              onClick={() =>
                void perform("resolve_approval", {
                  task_id: task.id,
                  approval_id: pending.id,
                  approved: false,
                })
              }
            >
              <X size={16} />
              Cancel
            </button>
            <button
              className="primary"
              disabled={busy}
              onClick={() =>
                void perform("resolve_approval", {
                  task_id: task.id,
                  approval_id: pending.id,
                  approved: true,
                })
              }
            >
              <Check size={16} />
              Approve {pending.spec.name}
            </button>
          </div>
        </section>
      )}
      {task.status === "waiting_for_user" && (
        <form
          className="answer-form"
          onSubmit={(e) => {
            e.preventDefault();
            void perform("answer_question", { task_id: task.id, answer });
          }}
        >
          <label htmlFor={`answer-${task.id}`}>Your answer</label>
          <textarea
            id={`answer-${task.id}`}
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
            maxLength={4000}
          />
          <button className="primary" disabled={busy || !answer.trim()}>
            Reply
          </button>
        </form>
      )}
      {(task.status === "waiting_for_auth" ||
        task.status === "waiting_for_resolution") && (
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void perform("cancel_task", { task_id: task.id })}
        >
          Cancel task
        </button>
      )}
      {task.status === "waiting_for_auth" && (
        <section className="auth-required" aria-label="Connection required">
          <p>
            Connect the requested service, then retry this same task. No action
            will be replayed while authorization is incomplete.
          </p>
          {onOpenSettings && (
            <button className="primary" type="button" onClick={onOpenSettings}>
              Open connections
            </button>
          )}
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void perform("resume_auth", { task_id: task.id })}
          >
            <RefreshCw size={16} /> Retry after connecting
          </button>
        </section>
      )}
    </article>
  );
}

function OutputView({ output }: { output: AssistantOutput }) {
  return (
    <section className="assistant-output" aria-label="Assistant result">
      {output.blocks.map((block) => {
        if (block.type === "markdown")
          return <Markdown key={block.id} text={block.markdown} />;
        if (block.type === "list") {
          const List = block.ordered ? "ol" : "ul";
          return (
            <List key={block.id}>
              {block.items.map((item, index) => (
                <li key={`${block.id}-${index}`}>{item}</li>
              ))}
            </List>
          );
        }
        if (block.type === "sources")
          return (
            <section className="output-sources" key={block.id}>
              <div className="source-heading">
                <h3>Sources</h3>
                <small>
                  Retrieved{" "}
                  {new Date(block.retrieved_at * 1000).toLocaleString()}
                </small>
              </div>
              {block.partial && (
                <p className="source-warning">
                  Some search results were omitted.
                </p>
              )}
              {!block.citations_resolved && (
                <p className="source-warning">
                  The answer has no resolved source citation.
                </p>
              )}
              {block.sources.map((source) => (
                <article className="source-card" key={source.id}>
                  <h4>{source.title}</h4>
                  <a href={source.url} target="_blank" rel="noreferrer">
                    Open source
                  </a>
                  <p>{source.excerpt}</p>
                  <details>
                    <summary>Source details</summary>
                    <small>[source:{source.id}]</small>
                  </details>
                </article>
              ))}
            </section>
          );
        return (
          <div className="output-table-scroll" key={block.id}>
            <table>
              <thead>
                <tr>
                  {block.columns.map((column, columnIndex) => (
                    <th key={`${block.id}-${columnIndex}`} scope="col">
                      {column}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {block.rows.map((row, rowIndex) => (
                  <tr key={`${block.id}-${rowIndex}`}>
                    {row.map((cell, cellIndex) => (
                      <td key={`${block.id}-${rowIndex}-${cellIndex}`}>
                        {cell}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      })}
    </section>
  );
}

function eventLabel(kind: RunEventKind | string) {
  return (
    {
      run_started: "Started",
      task_state: "Progress",
      worker_started: "Worker started",
      text_delta: "Text received",
      worker_terminal: "Worker finished",
      output_upsert: "Result updated",
      run_paused: "Paused",
      run_terminal: "Finished",
    }[kind] ?? kind.replaceAll("_", " ")
  );
}

function workerActivity(events: RunEventEnvelope[]) {
  const workers = new Map<string, { id: string; status: string }>();
  for (const event of [...events].sort(
    (left, right) => left.sequence - right.sequence,
  )) {
    if (!event.worker_id) continue;
    workers.set(event.worker_id, {
      id: event.worker_id,
      status:
        event.kind === "worker_terminal"
          ? event.message.toLowerCase().includes("failed")
            ? "Failed"
            : event.message.toLowerCase().includes("cancel")
              ? "Cancelled"
              : "Completed"
          : event.kind === "worker_started"
            ? "Running"
            : event.kind === "text_delta"
              ? "Running"
              : (workers.get(event.worker_id)?.status ?? "Queued"),
    });
  }
  return [...workers.values()];
}

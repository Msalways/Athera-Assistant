import { useState } from "react";
import { command } from "./service";
import type { Message, RuleRevision } from "./types";

export function PreferenceFeedback({ message }: { message: Message }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [usage, setUsage] = useState<RuleRevision[]>([]);
  async function save(remember: boolean) {
    setBusy(true);
    setError("");
    try {
      await command("record_observation", {
        conversation_id: message.conversation_id,
        source_id: message.id,
        kind: remember ? "remember" : "correction",
        text: text.trim(),
        confirmed: remember,
      });
      setText("");
      setNotice(
        remember
          ? "Preference saved for this conversation. Undo is available in Settings."
          : "Correction saved as evidence. Any inferred preference will wait for review.",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save feedback.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <details>
      <summary>Feedback and preferences</summary>
      <label>
        What should change?
        <textarea
          aria-label="Preference feedback"
          value={text}
          maxLength={2000}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <button disabled={busy || !text.trim()} onClick={() => void save(false)}>
        Save correction
      </button>
      <button disabled={busy || !text.trim()} onClick={() => void save(true)}>
        Remember this preference
      </button>
      <button
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setError("");
          try {
            setUsage(
              await command<RuleRevision[]>("personal_usage", {
                run_id: message.id,
              }),
            );
          } catch (e) {
            setError(
              e instanceof Error ? e.message : "Could not load preferences.",
            );
          } finally {
            setBusy(false);
          }
        }}
      >
        Show preferences used
      </button>
      {usage.map((r) => (
        <p key={`${r.rule.id}:${r.rule.version}`}>
          {r.rule.instruction} (revision {r.rule.version})
        </p>
      ))}
      {notice && <p role="status">{notice}</p>}
      {error && <p role="alert">{error}</p>}
    </details>
  );
}

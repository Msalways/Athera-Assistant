import { useEffect, useState } from "react";
import { command } from "./service";

export interface SmsSnapshot {
  session: {
    id: string;
    status: string;
    recipient: string;
    instruction: string;
    message: string;
    detail: string;
    latency_ms: number;
    steps: number;
  };
  device: {
    build?: string;
    android_api?: number;
    model?: string;
    abi?: string;
    enabled: boolean;
    package: string | null;
    stopped: boolean;
    reason: string;
  };
}
export default function SmsExperiment() {
  const [snapshot, setSnapshot] = useState<SmsSnapshot | null>(null);
  const [recipient, setRecipient] = useState("");
  const [instruction, setInstruction] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let alive = true;
    const refresh = async () => {
      try {
        const next = await command<SmsSnapshot>("sms_snapshot");
        if (alive) setSnapshot(next);
      } catch {
        if (alive)
          setError(
            "Cannot reach the Android experiment. Retry after reopening the app.",
          );
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 750);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  async function act(name: string, payload = {}) {
    setError("");
    setBusy(true);
    try {
      await command(name, payload);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const session = snapshot?.session;
  const active =
    busy || session?.status === "drafting" || session?.status === "running";
  return (
    <div className="app-shell sms-experiment">
      <header className="topbar">
        <div>
          <h1>Needle SMS experiment</h1>
          <p>On-device inference · No cloud fallback</p>
        </div>
      </header>
      <main className="page">
        <p>
          Draft a message, review the wording, then supervise your SMS app. Test
          with a recipient you control.
        </p>
        <p>SMS app: {snapshot?.device.package || "No default SMS app found"}</p>
        <p>
          Accessibility: {snapshot?.device.enabled ? "Enabled" : "Disabled"}
        </p>
        <button onClick={() => void act("sms_settings")} disabled={active}>
          Open accessibility settings
        </button>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void act("sms_draft", { recipient, instruction });
          }}
        >
          <label htmlFor="sms-recipient">
            Recipient number (include country code)
          </label>
          <input
            id="sms-recipient"
            type="tel"
            autoComplete="off"
            placeholder="+15551234567"
            pattern="\+?[0-9]{7,15}"
            required
            maxLength={16}
            value={recipient}
            disabled={active}
            onChange={(e) => setRecipient(e.target.value)}
          />
          <label htmlFor="sms-instruction">Instruction for Needle</label>
          <textarea
            id="sms-instruction"
            rows={4}
            required
            maxLength={2000}
            value={instruction}
            disabled={active}
            placeholder="Write a warm message saying I will arrive ten minutes late."
            onChange={(e) => setInstruction(e.target.value)}
          />
          <button type="submit" disabled={active || !snapshot}>
            Ask Needle to draft
          </button>
        </form>
        <section aria-live="polite" aria-label="Execution progress">
          <h2>{session?.status.replaceAll("_", " ") || "Connecting"}</h2>
          <p>{session?.detail}</p>
          {snapshot?.device.reason && <p>{snapshot.device.reason}</p>}
          <p>
            Measured inference: {session?.latency_ms ?? 0} ms · Steps:{" "}
            {session?.steps ?? 0}/8
          </p>
        </section>
        {session?.message && (
          <section className="sms-review" aria-label="Exact message approval">
            <h2>Needle’s proposed draft</h2>
            <p>
              To: <strong>{session.recipient}</strong>
            </p>
            <p className="sms-message">{session.message}</p>
            <p>
              Approval allows one send attempt of this exact message to this
              exact number. The app will stop if it cannot verify both.
            </p>
            <button
              disabled={
                active ||
                session.status !== "awaiting_approval" ||
                !snapshot?.device.enabled ||
                !snapshot.device.package
              }
              onClick={() =>
                void act("sms_approve", {
                  id: session.id,
                  recipient: session.recipient,
                  message: session.message,
                })
              }
            >
              Approve exact message and start SMS session
            </button>
          </section>
        )}
        {error && (
          <p role="alert" className="error-banner">
            {error}
          </p>
        )}
        <p>
          After a send attempt, check the SMS app before trying again. Delivery
          is not automatically verified.
        </p>
        <details>
          <summary>Test report to share</summary>
          <p>
            Copy this report after testing. It excludes your recipient,
            instruction, message, and screen content.
          </p>
          <textarea
            aria-label="Test report"
            readOnly
            rows={12}
            value={JSON.stringify(
              {
                device: snapshot?.device,
                status: session?.status,
                detail: session?.detail,
                inference_ms: session?.latency_ms,
                steps: session?.steps,
                error,
              },
              null,
              2,
            )}
            onFocus={(e) => e.target.select()}
          />
        </details>
        <details>
          <summary>Licenses and test limits</summary>
          <p>
            Needle 2, Cactus Compute, Apache-2.0. Runtime and model are bundled.
            Physical-device startup, offline inference, memory, latency,
            accessibility compatibility and sending remain unverified.
          </p>
          <a href="/needle-LICENSE.txt">Needle license</a>
          <p>
            <a href="/android-runtime-NOTICE.txt">
              Android C++ runtime notices
            </a>
          </p>
        </details>
      </main>
      <footer className="sms-stop">
        <button
          onClick={() => {
            void command("sms_stop").catch((e) => setError(String(e)));
          }}
        >
          Stop
        </button>
        <small>The SMS app also shows a Stop overlay during execution.</small>
      </footer>
    </div>
  );
}

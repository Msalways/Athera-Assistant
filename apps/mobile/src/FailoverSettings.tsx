import { useEffect, useMemo, useRef, useState } from "react";
import { clearFailoverPolicy, setFailoverPolicy } from "./service";
import type { FailoverMember, SavedProviderProfile } from "./types";

const SCHEMA = "aethra.failover-policy.v1";

function nameOf(profiles: SavedProviderProfile[], providerId: string): string {
  return (
    profiles.find((entry) => entry.profile.provider_id === providerId)?.profile
      .display_name ?? providerId
  );
}

export function FailoverSettings({
  profiles,
  savedChain = [],
  loadProfiles = () => Promise.resolve<SavedProviderProfile[]>([]),
}: {
  profiles: SavedProviderProfile[];
  /** The order the runtime is actually using, so reopening shows it unchanged. */
  savedChain?: FailoverMember[];
  loadProfiles?: () => Promise<unknown>;
}) {
  const [primary, setPrimary] = useState("");
  const [fallback, setFallback] = useState("");
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  // Only providers with a stored key can answer, so only they can join a chain.
  const eligible = useMemo(
    () => profiles.filter((entry) => entry.key_configured),
    [profiles],
  );
  const fallbackOptions = useMemo(
    () => eligible.filter((entry) => entry.profile.provider_id !== primary),
    [eligible, primary],
  );

  // Track which chain these selectors were last filled from, rather than a
  // one-shot flag. A one-shot flag latched whatever arrived first, so opening
  // Settings before the runtime had reported its chain showed the merely-active
  // provider and never corrected itself. Re-syncing on change fixes the late
  // arrival as well as the reopen.
  const hydratedFor = useRef<string | null>(null);
  const savedKey = savedChain.map((member) => member.provider_id).join(">");

  useEffect(() => {
    if (savedChain.length > 0) {
      if (hydratedFor.current === savedKey) return;
      hydratedFor.current = savedKey;
      setPrimary(savedChain[0].provider_id);
      setFallback(savedChain[1]?.provider_id ?? "");
      return;
    }
    // No chain configured: fall back to the active provider, but only once the
    // profiles have actually arrived, or there would be nothing to choose from.
    if (hydratedFor.current === "defaults") return;
    const active = eligible.find((entry) => entry.active) ?? eligible[0];
    if (!active) return;
    hydratedFor.current = "defaults";
    setPrimary(active.profile.provider_id);
  }, [eligible, savedChain, savedKey]);

  async function run(action: () => Promise<boolean>, message: string) {
    setBusy(true);
    setError("");
    setStatus("");
    try {
      await action();
      await loadProfiles();
      setStatus(message);
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Could not update provider fallback.",
      );
    } finally {
      setBusy(false);
    }
  }

  const canSave = Boolean(primary && fallback && primary !== fallback);

  if (eligible.length < 2) {
    return (
      <section
        aria-label="Provider fallback"
        className="settings-form failover-settings"
      >
        <h3>Provider fallback</h3>
        <p className="field-help">
          {eligible.length === 0
            ? "Add a provider with a stored key to enable fallback."
            : "Add a second provider with a stored key to enable fallback."}
        </p>
        <p className="field-help">
          Athera tries one alternative only when the first provider is
          unreachable or overloaded. It never switches after a key, model, or
          endpoint problem, because that would hide a mistake in your setup.
        </p>
      </section>
    );
  }

  return (
    <section
      aria-label="Provider fallback"
      className="settings-form failover-settings"
    >
      <h3>Provider fallback</h3>
      <p className="field-help">
        Athera tries one alternative only when the first provider is
        unreachable, overloaded, or out of quota. It never switches after a key,
        model, or endpoint problem, because that would hide a mistake in your
        setup, and it never switches after it has already started writing an
        answer.
      </p>
      <label>
        Try first
        <select
          value={primary}
          disabled={busy}
          onChange={(e) => setPrimary(e.target.value)}
        >
          {eligible.map((entry) => (
            <option
              key={entry.profile.provider_id}
              value={entry.profile.provider_id}
            >
              {entry.profile.display_name ?? entry.profile.provider_id}
              {entry.active ? " (active)" : ""}
            </option>
          ))}
        </select>
      </label>
      <label>
        Then try
        <select
          value={fallback}
          disabled={busy}
          onChange={(e) => setFallback(e.target.value)}
        >
          <option value="">Select a fallback…</option>
          {fallbackOptions.map((entry) => (
            <option
              key={entry.profile.provider_id}
              value={entry.profile.provider_id}
            >
              {entry.profile.display_name ?? entry.profile.provider_id}
            </option>
          ))}
        </select>
      </label>
      <p className="field-help">
        Both providers use their own stored key. A key is never reused to reach
        another host.
      </p>
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
      <div className="button-row">
        <button
          className="primary"
          type="button"
          disabled={!canSave || busy}
          onClick={() =>
            void run(
              () =>
                setFailoverPolicy({
                  schema: SCHEMA,
                  primary_provider_id: primary,
                  fallback_provider_ids: [fallback],
                }),
              `Fallback enabled: ${nameOf(eligible, primary)}, then ${nameOf(
                eligible,
                fallback,
              )}.`,
            )
          }
        >
          Save fallback order
        </button>
        <button
          className="secondary"
          type="button"
          disabled={busy}
          onClick={() => {
            setFallback("");
            void run(clearFailoverPolicy, "Provider fallback turned off.");
          }}
        >
          Turn off fallback
        </button>
      </div>
    </section>
  );
}

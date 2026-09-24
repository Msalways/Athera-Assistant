import { useState } from "react";
import { command } from "./service";
import type { AdaptiveRule, RuleProposal, RuleRevision } from "./types";

export function AdaptiveRulesSettings({
  proposals: initial,
  rules: initialRules,
}: {
  proposals: RuleProposal[];
  rules: AdaptiveRule[];
}) {
  const [proposals, setProposals] = useState(initial);
  const [rules, setRules] = useState(initialRules);
  const [instruction, setInstruction] = useState("");
  const [key, setKey] = useState("");
  const [replacement, setReplacement] = useState<AdaptiveRule | null>(null);
  const [history, setHistory] = useState<RuleRevision[]>([]);
  const [selected, setSelected] = useState<AdaptiveRule | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [learning, setLearning] = useState<boolean | null>(null);

  async function refresh() {
    const [nextProposals, nextRules] = await Promise.all([
      command<RuleProposal[]>("list_rule_proposals"),
      command<AdaptiveRule[]>("list_adaptive_rules"),
    ]);
    setProposals(nextProposals);
    setRules(nextRules);
  }
  async function run(operation: () => Promise<void>) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await operation();
      await refresh();
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : "Could not save this change. Refresh and try again.",
      );
    } finally {
      setBusy(false);
    }
  }
  function scope(rule: AdaptiveRule | null): string {
    if (!rule || rule.scope === "global") return "global";
    if ("conversation" in rule.scope)
      return `conversation:${rule.scope.conversation}`;
    return `workflow:${rule.scope.workflow}`;
  }
  async function decide(proposal: RuleProposal, approved: boolean) {
    await command("review_rule_proposal", {
      proposal_id: proposal.id,
      expected_version: proposal.rule.version,
      approved,
      confirmed: true,
    });
    setNotice(approved ? "Preference enabled." : "Suggestion rejected.");
  }
  return (
    <section className="memory-settings">
      <h3>Preferences and learning</h3>
      <p>
        Your saved preferences apply to conversations and actions. Inferred
        suggestions wait for review.
      </p>
      <form
        className="settings-form"
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            await command("remember_preference", {
              instruction: instruction.trim(),
              preference_key: key.trim() || null,
              scope: scope(replacement),
              supersedes: replacement?.id ?? null,
              confirmed: true,
            });
            setInstruction("");
            setKey("");
            setReplacement(null);
            setNotice(
              "Preference saved and active. You can undo it from its history.",
            );
          });
        }}
      >
        <label>
          {replacement ? "Replace preference" : "Remember this preference"}
          <textarea
            aria-label="Adaptive rule"
            value={instruction}
            maxLength={2000}
            onChange={(e) => setInstruction(e.target.value)}
          />
        </label>
        <label>
          Preference category (optional)
          <input
            value={key}
            maxLength={128}
            placeholder="response.length"
            onChange={(e) => setKey(e.target.value)}
          />
        </label>
        <button className="primary" disabled={busy || !instruction.trim()}>
          Save and remember
        </button>
        {replacement && (
          <button
            type="button"
            onClick={() => {
              setReplacement(null);
              setInstruction("");
              setKey("");
            }}
          >
            Cancel replacement
          </button>
        )}
      </form>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <button disabled={busy} onClick={() => void run(refresh)}>
        Refresh preferences
      </button>
      <details>
        <summary>Learning controls</summary>
        <p>
          Generating suggestions sends saved corrections to your configured
          action model. Temporary chats are excluded.
        </p>
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const status = await command<{ enabled: boolean }>(
                "learning_settings",
              );
              setLearning(status.enabled);
            })
          }
        >
          Check automatic learning
        </button>
        {learning !== null && (
          <label>
            <input
              type="checkbox"
              checked={learning}
              disabled={busy}
              onChange={(e) => {
                const enabled = e.target.checked;
                void run(async () => {
                  await command("learning_settings", {
                    enabled,
                    confirmed: true,
                  });
                  setLearning(enabled);
                });
              }}
            />
            Generate suggestions automatically
          </label>
        )}
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const created = await command<RuleProposal[]>(
                "process_learning",
                { confirmed: true },
              );
              setNotice(
                `${created.length} suggestions generated. Review them below.`,
              );
            })
          }
        >
          Generate suggestions from corrections
        </button>
      </details>
      {proposals
        .filter((p) => p.rule.status === "proposed")
        .map((p) => (
          <div className="memory-row" key={p.id}>
            <div>
              <strong>{p.rule.instruction}</strong>
              <p>{p.rationale}</p>
              <small>
                Suggested · {p.rule.evidence_ids.length} evidence records
              </small>
            </div>
            <RuleEvidence ruleId={p.rule.id} />
            <button
              disabled={busy}
              onClick={() => void run(() => decide(p, true))}
            >
              Enable
            </button>
            <button
              disabled={busy}
              onClick={() => void run(() => decide(p, false))}
            >
              Reject
            </button>
          </div>
        ))}
      {rules.map((rule) => (
        <div className="memory-row" key={rule.id}>
          <div>
            <strong>{rule.instruction}</strong>
            <small>Active · revision {rule.version}</small>
          </div>
          <button
            disabled={busy}
            onClick={() => {
              setReplacement(rule);
              setInstruction(rule.instruction);
              setKey(rule.preference_key ?? "");
            }}
          >
            Replace
          </button>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await command("disable_adaptive_rule", {
                  rule_id: rule.id,
                  expected_version: rule.version,
                  confirmed: true,
                });
                setNotice("Preference disabled.");
              })
            }
          >
            Disable
          </button>
        </div>
      ))}
      <details>
        <summary>History and undo</summary>
        {proposals.map((p) => (
          <button
            disabled={busy}
            key={p.id}
            onClick={() =>
              void run(async () => {
                setHistory(
                  await command<RuleRevision[]>("rule_history", {
                    rule_id: p.rule.id,
                  }),
                );
                setSelected(p.rule);
              })
            }
          >
            {p.rule.instruction} ({p.rule.status})
          </button>
        ))}
        {history.map((revision) => (
          <div key={revision.rule.version}>
            <p>
              Revision {revision.rule.version}: {revision.decision} —{" "}
              {revision.rule.instruction}
            </p>
            {revision.historical_baseline && (
              <small>Imported baseline; earlier history is unavailable.</small>
            )}
            {selected &&
              revision.rule.status === "enabled" &&
              revision.rule.version < selected.version && (
                <button
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await command("rollback_rule", {
                        rule_id: selected.id,
                        expected_version: selected.version,
                        version: revision.rule.version,
                        confirmed: true,
                      });
                      setHistory([]);
                      setSelected(null);
                      setNotice("Earlier preference restored.");
                    })
                  }
                >
                  Restore revision {revision.rule.version}
                </button>
              )}
          </div>
        ))}
      </details>
    </section>
  );
}

function RuleEvidence({ ruleId }: { ruleId: string }) {
  const [detail, setDetail] = useState<{
    evidence: { id: string; text: string; kind: string }[];
    skill: null | {
      evaluation: {
        passed: boolean;
        cases: number;
        baseline_matches: number;
        candidate_matches: number;
        failures: string[];
      };
    };
  } | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <details>
      <summary>Evidence and evaluation</summary>
      <button
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setError("");
          try {
            setDetail(
              await command("personal_rule_details", { rule_id: ruleId }),
            );
          } catch (e) {
            setError(
              e instanceof Error ? e.message : "Could not load evidence.",
            );
          } finally {
            setBusy(false);
          }
        }}
      >
        Load supporting evidence
      </button>
      {error && <p role="alert">{error}</p>}
      {detail?.evidence.map((e) => (
        <p key={e.id}>
          {e.kind}: {e.text}
        </p>
      ))}
      {detail?.skill && (
        <p>
          {detail.skill.evaluation.cases === 0
            ? "This workflow needs recorded-case evaluation before it can be enabled."
            : `Evaluation: ${detail.skill.evaluation.candidate_matches}/${detail.skill.evaluation.cases} cases matched; baseline ${detail.skill.evaluation.baseline_matches}/${detail.skill.evaluation.cases}. ${detail.skill.evaluation.passed ? "Ready for review." : "Not ready to enable."}`}
        </p>
      )}
    </details>
  );
}

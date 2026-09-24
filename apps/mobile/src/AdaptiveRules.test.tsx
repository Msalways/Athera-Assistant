import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AdaptiveRulesSettings } from "./AdaptiveRules";
import { PreferenceFeedback } from "./PreferenceFeedback";
import { command } from "./service";
import type { RuleProposal } from "./types";

vi.mock("./service", () => ({ command: vi.fn() }));
const proposal: RuleProposal = {
  schema: "aethra.rule-proposal.v1",
  id: "proposal",
  rationale: "User preference",
  proposed_at: 1,
  rule: {
    schema: "aethra.adaptive-rule.v1",
    id: "rule",
    version: 1,
    scope: "global",
    status: "proposed",
    source: "user",
    priority: 50,
    instruction: "Be concise",
    evidence_ids: [],
    created_at: 1,
    updated_at: 1,
    expires_at: null,
    supersedes: null,
  },
};
beforeEach(() => vi.mocked(command).mockReset());

it("refreshes after review and binds the decision to the displayed revision", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "list_rule_proposals")
      return [
        {
          ...proposal,
          rule: { ...proposal.rule, status: "enabled", version: 2 },
        },
      ] as never;
    if (name === "list_adaptive_rules")
      return [{ ...proposal.rule, status: "enabled", version: 2 }] as never;
    return null as never;
  });
  render(<AdaptiveRulesSettings proposals={[proposal]} rules={[]} />);
  fireEvent.click(screen.getByRole("button", { name: "Enable" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Disable" })).toBeEnabled(),
  );
  expect(command).toHaveBeenCalledWith("review_rule_proposal", {
    proposal_id: "proposal",
    expected_version: 1,
    approved: true,
    confirmed: true,
  });
  expect(
    screen.queryByRole("button", { name: "Enable" }),
  ).not.toBeInTheDocument();
});

it("shows a recoverable save failure without losing the preference", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "remember_preference") throw new Error("Save failed");
    if (name === "list_rule_proposals" || name === "list_adaptive_rules") {
      return [] as never;
    }
    return null as never;
  });
  render(<AdaptiveRulesSettings proposals={[]} rules={[]} />);
  fireEvent.change(screen.getByLabelText("Adaptive rule"), {
    target: { value: "Use metric units" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save and remember" }));
  await waitFor(() =>
    expect(screen.getByRole("alert")).toHaveTextContent("Save failed"),
  );
  expect(screen.getByLabelText("Adaptive rule")).toHaveValue(
    "Use metric units",
  );
});

it("records corrections separately from explicit remember instructions", async () => {
  vi.mocked(command).mockResolvedValue(null);
  render(
    <PreferenceFeedback
      message={{
        id: "message",
        conversation_id: "conversation",
        role: "assistant",
        content: "Hello",
        status: "complete",
        created_at: 1,
      }}
    />,
  );
  fireEvent.click(screen.getByText("Feedback and preferences"));
  fireEvent.change(screen.getByLabelText("Preference feedback"), {
    target: { value: "Use shorter answers" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save correction" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith(
      "record_observation",
      expect.objectContaining({
        kind: "correction",
        confirmed: false,
        source_id: "message",
      }),
    ),
  );
  await waitFor(() =>
    expect(screen.getByLabelText("Preference feedback")).toHaveValue(""),
  );
  fireEvent.change(screen.getByLabelText("Preference feedback"), {
    target: { value: "Prefer metric units" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Remember this preference" }),
  );
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith(
      "record_observation",
      expect.objectContaining({ kind: "remember", confirmed: true }),
    ),
  );
});

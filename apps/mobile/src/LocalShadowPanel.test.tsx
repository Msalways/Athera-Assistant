import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { Configuration } from "./Configuration";
import type { LocalShadowState, Snapshot } from "./types";

const base: LocalShadowState = {
  provisioned: true,
  observations: 0,
  judgeable: 0,
  agreed: 0,
  abstained: 0,
  uncalibrated: 0,
  agreement_rate: null,
  gated_agreement_rate: null,
  gated_judgeable: 0,
  gated_agreed: 0,
  threshold: 0.4,
};

function openDiagnostics(
  opts: { needle?: string; local_shadow?: LocalShadowState } = {},
) {
  const snapshot = {
    tasks: [],
    capabilities: [],
    needle: opts.needle ?? "not_linked",
    voice: "deferred",
    cloud_credential: "missing",
    cloud_session_key: false,
    connections: [],
    settings: { cloud: null },
    ...(opts.local_shadow ? { local_shadow: opts.local_shadow } : {}),
  } as unknown as Snapshot;
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  fireEvent.click(screen.getByRole("tab", { name: "Diagnostics" }));
}

describe("On-device model reporting", () => {
  it("never says a local model is present when it is not linked", () => {
    openDiagnostics({ needle: "not_linked", local_shadow: base });
    // The observer is running while the bundled model is absent. Saying plain
    // "Needle: not_linked" next to a live observer reads as a contradiction.
    expect(screen.getByText("Bundled on-device model")).toBeTruthy();
    expect(screen.getByText("Not linked")).toBeTruthy();
    expect(screen.queryByText("Needle")).toBeNull();
  });

  it("separates the two on-device components in words", () => {
    openDiagnostics({ needle: "not_linked", local_shadow: base });
    expect(screen.getByText("On-device observer")).toBeTruthy();
    expect(
      screen.getByText(
        /It cannot answer: every reply you have received came from the cloud model/i,
      ),
    ).toBeTruthy();
  });

  it("does not claim a version it is not running", () => {
    openDiagnostics({ needle: "ready" });
    expect(screen.queryByText(/Needle 2/)).toBeNull();
    expect(screen.getByText("Linked")).toBeTruthy();
  });

  it("says plainly when nothing is being measured", () => {
    openDiagnostics({ needle: "not_linked" });
    expect(
      screen.getByText(/every answer you get is the cloud model's/i),
    ).toBeTruthy();
  });

  it("refuses to invent an agreement rate before there is data", () => {
    openDiagnostics({ local_shadow: { ...base, observations: 12 } });
    expect(screen.getByText("Turns observed")).toBeTruthy();
    expect(screen.getAllByText(/not enough data/i).length).toBeGreaterThan(0);
  });

  it("marks the numbers as a measurement rather than a result", () => {
    openDiagnostics({ local_shadow: { ...base, observations: 4 } });
    expect(screen.getByText(/a measurement, not a result/i)).toBeTruthy();
  });
});

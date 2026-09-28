import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import { command, CommandTimeoutError } from "./service";

vi.mock("./service", () => ({
  command: vi.fn(),
  CommandTimeoutError: class extends Error {},
}));

const snapshot = {
  tasks: [],
  capabilities: [],
  cloud_credential: "configured",
  cloud_session_key: false,
  settings: { cloud: { model: "action-model" }, connections: [] },
  needle: "ready",
  voice: "deferred",
};

const emptyRunEvents = {
  schema: "aethra.run-events.v1",
  after: 0,
  next_after: 0,
  has_more: false,
  reset_required: false,
  events: [],
};

beforeEach(() => {
  window.localStorage.clear();
  vi.clearAllMocks();
});

it("defaults to single assistant mode without requiring an offline model", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "model_status"
      ? { availability: "missing_model", installation: null, manifest: null }
      : name === "build_info"
        ? {
            git_hash: "test",
            git_dirty: false,
            build_timestamp: 0,
            version: "0.1.0",
          }
        : name === "run_events"
          ? emptyRunEvents
          : snapshot,
  );
  render(<App />);
  expect(await screen.findByText("Ready")).toBeVisible();
  expect(
    screen.queryByText("Download offline chat model"),
  ).not.toBeInTheDocument();
});

it("stays in a starting state through a slow cold start instead of failing", async () => {
  let calls = 0;
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "run_events") return emptyRunEvents;
    if (name === "model_status")
      return {
        availability: "missing_model",
        installation: null,
        manifest: null,
      };
    calls += 1;
    // A cold runtime (boot, Keystore, SQLite) can take far longer than a
    // steady-state poll. The first attempts must not be reported as failure.
    if (calls <= 2) {
      throw new CommandTimeoutError("snapshot", 25000);
    }
    return snapshot;
  });
  render(<App />);
  expect(await screen.findByText("Starting Athera")).toBeVisible();
  await waitFor(() => expect(screen.getByText("Ready")).toBeVisible(), {
    timeout: 6000,
  });
  expect(screen.queryByText(/did not respond within/i)).not.toBeInTheDocument();
});

it("guides a first-time user to configure a provider before sending", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "run_events"
      ? emptyRunEvents
      : { ...snapshot, cloud_credential: "missing" },
  );
  render(<App />);
  expect(await screen.findByText("Connect a provider to start")).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Configure provider" }),
  ).toBeVisible();
  expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  expect(
    screen.getByText("Connect a provider to enable conversation."),
  ).toBeVisible();
});

it("resumes the stored conversation instead of creating a new one", async () => {
  window.localStorage.setItem(
    "athera.active_conversation_id",
    "conversation-fixture",
  );
  vi.mocked(command).mockImplementation(async (name) =>
    name === "run_events"
      ? emptyRunEvents
      : name === "model_status"
        ? { availability: "ready", installation: null, manifest: null }
        : snapshot,
  );
  render(<App />);
  await screen.findByText("Ready");
  fireEvent.change(screen.getByLabelText("Ask Athera"), {
    target: { value: "Continue" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith(
      "submit_input",
      expect.objectContaining({ conversation_id: "conversation-fixture" }),
      { timeoutMs: 10000 },
    ),
  );
});

it("restores an unsent composer draft after a relaunch", async () => {
  window.localStorage.setItem(
    "athera.composer_draft",
    "Draft from before restart",
  );
  vi.mocked(command).mockImplementation(async (name) =>
    name === "run_events" ? emptyRunEvents : snapshot,
  );
  render(<App />);
  expect(await screen.findByLabelText("Ask Athera")).toHaveValue(
    "Draft from before restart",
  );
});

it("sends requests via submit_input on the single composer", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "model_status"
      ? { availability: "ready", installation: null, manifest: null }
      : name === "build_info"
        ? {
            git_hash: "test",
            git_dirty: false,
            build_timestamp: 0,
            version: "0.1.0",
          }
        : name === "run_events"
          ? emptyRunEvents
          : snapshot,
  );
  render(<App />);
  await screen.findByText("Ready");
  fireEvent.change(screen.getByLabelText("Ask Athera"), {
    target: { value: "Send the report" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith(
      "submit_input",
      {
        conversation_id: expect.any(String),
        text: "Send the report",
        source: "text",
      },
      { timeoutMs: 10000 },
    ),
  );
  expect(command).not.toHaveBeenCalledWith("send_message", expect.anything());
});

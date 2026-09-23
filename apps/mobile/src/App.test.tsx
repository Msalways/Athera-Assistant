import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import App from "./App";
import { command } from "./service";

vi.mock("./service", () => ({ command: vi.fn() }));

const snapshot = {
  tasks: [],
  capabilities: [],
  cloud_credential: "configured",
  cloud_session_key: false,
  settings: { cloud: { model: "action-model" }, connections: [] },
  needle: "ready",
  voice: "deferred",
};

it("defaults to single assistant mode without requiring an offline model", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "model_status"
      ? { availability: "missing_model", installation: null, manifest: null }
      : name === "build_info"
        ? { git_hash: "test", git_dirty: false, build_timestamp: 0, version: "0.1.0" }
        : snapshot,
  );
  render(<App />);
  expect(await screen.findByText("Ready")).toBeVisible();
  expect(
    screen.queryByText("Download offline chat model"),
  ).not.toBeInTheDocument();
});

it("sends requests via submit_input on the single composer", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "model_status"
      ? { availability: "ready", installation: null, manifest: null }
      : name === "build_info"
        ? { git_hash: "test", git_dirty: false, build_timestamp: 0, version: "0.1.0" }
        : snapshot,
  );
  render(<App />);
  await screen.findByText("Ready");
  fireEvent.change(screen.getByLabelText("Ask Athera"), {
    target: { value: "Send the report" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("submit_input", {
      conversation_id: expect.any(String),
      text: "Send the report",
      source: "text",
    }),
  );
  expect(command).not.toHaveBeenCalledWith("send_message", expect.anything());
});

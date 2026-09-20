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

it("surfaces missing local model and opens its setup", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "model_status"
      ? { availability: "missing_model", installation: null, manifest: null }
      : snapshot,
  );
  render(<App />);
  expect(await screen.findByText("Local model needed")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Set up local model" }));
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeVisible();
});

it("keeps explicit action requests on submit_input", async () => {
  vi.mocked(command).mockImplementation(async (name) =>
    name === "model_status"
      ? { availability: "ready", installation: null, manifest: null }
      : snapshot,
  );
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "Action" }));
  fireEvent.change(screen.getByLabelText("Action request"), {
    target: { value: "Send the report" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send action" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("submit_input", {
      conversation_id: expect.any(String),
      text: "Send the report",
      source: "text",
    }),
  );
  expect(command).not.toHaveBeenCalledWith("send_message", expect.anything());
});

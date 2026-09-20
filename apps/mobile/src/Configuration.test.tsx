import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Configuration } from "./Configuration";
import { command } from "./service";
import type { Snapshot } from "./types";

vi.mock("./service", () => ({ command: vi.fn() }));
vi.mocked(command).mockResolvedValue({
  availability: "unavailable",
  installation: null,
  manifest: null,
});

const snapshot: Snapshot = {
  tasks: [],
  capabilities: [],
  needle: "ready",
  voice: "deferred",
  cloud_credential: "missing",
  cloud_session_key: false,
  settings: {
    cloud: {
      id: "test",
      endpoint: "https://example.com/v1",
      model: "fixture-model",
      secret_ref: "ASSISTANT_TEST_KEY",
      api: "chat_completions",
    },
    engine: {
      max_steps: 24,
      max_failures: 3,
      max_identical_calls: 2,
      timeout_seconds: 30,
      candidate_limit: 20,
      tool_limit: 8,
      fast_context_bytes: 16000,
      cloud_context_bytes: 48000,
    },
    connections: [],
  },
};

it("sends a masked key through its dedicated command and clears it after success", async () => {
  const act = vi.fn().mockResolvedValue(true);
  render(<Configuration snapshot={snapshot} view="settings" act={act} />);
  const field = screen.getByLabelText("API key (until backend restart)");
  expect(field).toHaveAttribute("type", "password");
  fireEvent.change(field, { target: { value: "fixture-not-real-key" } });
  fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
  await waitFor(() => expect(field).toHaveValue(""));
  expect(act).toHaveBeenCalledWith("save_cloud_provider", {
    cloud: snapshot.settings.cloud,
    api_key: "fixture-not-real-key",
  });
  expect(act).not.toHaveBeenCalledWith("save_settings", expect.anything());
});

it("keeps an unsaved key when the backend rejects configuration", async () => {
  const act = vi.fn().mockResolvedValue(false);
  render(<Configuration snapshot={snapshot} view="settings" act={act} />);
  const field = screen.getByLabelText("API key (until backend restart)");
  fireEvent.change(field, { target: { value: "fixture-not-real-key" } });
  fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Save provider" }),
    ).not.toBeDisabled(),
  );
  expect(field).toHaveValue("fixture-not-real-key");
});

it("can clear a backend session key without revealing it", async () => {
  const act = vi.fn().mockResolvedValue(true);
  render(
    <Configuration
      snapshot={{
        ...snapshot,
        cloud_session_key: true,
        cloud_credential: "configured",
      }}
      view="settings"
      act={act}
    />,
  );
  expect(screen.getByLabelText("API key (until backend restart)")).toHaveValue(
    "",
  );
  fireEvent.click(screen.getByRole("button", { name: "Clear session key" }));
  await waitFor(() => expect(act).toHaveBeenCalledWith("clear_cloud_key", {}));
});

it("connects the reviewed anonymous Parallel Search preset", async () => {
  const act = vi.fn().mockResolvedValue(true);
  render(<Configuration snapshot={snapshot} view="connections" act={act} />);
  expect(
    screen.getByText("No account needed.", { exact: false }),
  ).toBeVisible();
  fireEvent.click(
    screen.getByRole("button", { name: "Connect Parallel Search" }),
  );
  await waitFor(() =>
    expect(act).toHaveBeenCalledWith("connect_parallel_search", {}),
  );
});

it("passes a bearer credential through the dedicated session-only command", async () => {
  const act = vi.fn().mockResolvedValue(true);
  render(<Configuration snapshot={snapshot} view="connections" act={act} />);
  fireEvent.change(screen.getByLabelText("Name"), {
    target: { value: "Protected MCP" },
  });
  fireEvent.change(screen.getByLabelText("Server URL"), {
    target: { value: "https://mcp.example.com/tools" },
  });
  fireEvent.change(screen.getByLabelText("Authentication"), {
    target: { value: "bearer_token" },
  });
  fireEvent.change(screen.getByLabelText(/^Credential/), {
    target: { value: "fixture-secret" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  await waitFor(() =>
    expect(act).toHaveBeenCalledWith("connect_mcp_with_credential", {
      connection: expect.objectContaining({
        name: "Protected MCP",
        url: "https://mcp.example.com/tools",
        authentication: expect.objectContaining({
          kind: "bearer_token",
          secret_ref: expect.stringMatching(/^session:mcp:/),
        }),
      }),
      secret: "fixture-secret",
    }),
  );
});

it("starts generic OAuth discovery without putting a token in settings", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "start_mcp_oauth")
      return {
        transaction_id: "transaction",
        connection_id: "connection",
        authorization_url: "https://auth.example.com/authorize?state=opaque",
        expires_at: 123,
        requested_scopes: ["read"],
        resume_task_id: null,
      };
    return { availability: "unavailable", installation: null, manifest: null };
  });
  const act = vi.fn().mockResolvedValue(true);
  render(<Configuration snapshot={snapshot} view="connections" act={act} />);
  fireEvent.change(screen.getByLabelText("Name"), {
    target: { value: "OAuth MCP" },
  });
  fireEvent.change(screen.getByLabelText("Authentication"), {
    target: { value: "oauth_authorization_code" },
  });
  fireEvent.change(screen.getByLabelText("Server URL"), {
    target: { value: "https://mcp.example.com/api" },
  });
  fireEvent.change(screen.getByLabelText("Authorization server issuer"), {
    target: { value: "https://auth.example.com" },
  });
  fireEvent.change(screen.getByLabelText(/^Requested scopes/), {
    target: { value: "read" },
  });
  fireEvent.change(screen.getByLabelText("Redirect URI"), {
    target: { value: "https://app.example.com/oauth/callback" },
  });
  fireEvent.change(screen.getByLabelText("Pre-registered client ID"), {
    target: { value: "public-client" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  await waitFor(() =>
    expect(act).toHaveBeenCalledWith(
      "save_mcp_connection",
      expect.objectContaining({
        authentication: expect.objectContaining({
          kind: "oauth_authorization_code",
          token_ref: expect.stringMatching(/^keystore:oauth:/),
          requested_scopes: ["read"],
        }),
      }),
    ),
  );
  expect(command).toHaveBeenCalledWith("start_mcp_oauth", {
    connection_id: expect.any(String),
    client_id: "public-client",
    client_metadata_url: null,
    redirect_uri: "https://app.example.com/oauth/callback",
    resume_task_id: null,
  });
  expect(
    await screen.findByRole("link", { name: "Open secure sign-in" }),
  ).toHaveAttribute("href", "https://auth.example.com/authorize?state=opaque");
  expect(screen.queryByLabelText("Credential")).not.toBeInTheDocument();
});

it("requires explicit confirmation in the memory command", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "list_memories") return [];
    if (name === "save_memory")
      return { id: "memory", text: "Likes tea", updated_at: 1 };
    return { availability: "unavailable", installation: null, manifest: null };
  });
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
  fireEvent.change(screen.getByLabelText("Remember this"), {
    target: { value: "Likes tea" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Confirm and save" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("save_memory", {
      text: "Likes tea",
      confirmed: true,
    }),
  );
});

it("edits the existing memory with a stable accessible label", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "list_memories")
      return [{ id: "memory", text: "Likes tea", updated_at: 1 }];
    return {};
  });
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
  fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
  fireEvent.change(screen.getByLabelText("Edit memory", { exact: true }), {
    target: { value: "Likes coffee" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Confirm and save" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("save_memory", {
      id: "memory",
      text: "Likes coffee",
      confirmed: true,
    }),
  );
});

it("shows byte progress and cancels a user-started model setup", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "model_status")
      return {
        availability: "missing_model",
        manifest: {
          id: "local-fixture",
          size_bytes: 2 * 1024 * 1024,
          license: "test",
          context_tokens: 1024,
        },
        installation: {
          status: "downloading",
          downloaded_bytes: 1024 * 1024,
          error: null,
          manifest: {
            id: "local-fixture",
            size_bytes: 2 * 1024 * 1024,
            license: "test",
            context_tokens: 1024,
          },
        },
      };
    return {};
  });
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  expect(await screen.findByLabelText("Model download progress")).toHaveValue(
    50,
  );
  expect(screen.getByText("1.0 MB of 2.0 MB downloaded (50%)")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Cancel setup" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("cancel_model_download", {}),
  );
});

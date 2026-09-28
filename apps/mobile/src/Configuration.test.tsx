import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Configuration } from "./Configuration";
import { command } from "./service";
import type { Snapshot } from "./types";

vi.mock("./service", () => ({
  command: vi.fn(),
  getProviderCatalog: vi.fn(),
  saveProviderProfile: vi.fn(),
  listProviderProfiles: vi.fn(),
  deleteProviderProfile: vi.fn(),
  setActiveProvider: vi.fn(),
  testProviderConnection: vi.fn(),
  getBuildInfo: vi.fn(),
}));
import {
  deleteProviderProfile,
  getProviderCatalog,
  listProviderProfiles,
  saveProviderProfile,
  setActiveProvider,
  testProviderConnection,
} from "./service";
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
  connections: [],
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

const catalog = {
  schema: "aethra.provider-catalog-registry.v1",
  providers: [
    {
      schema: "aethra.provider-catalog.v1",
      id: "openai",
      display_name: "OpenAI",
      transport_family: "open_ai_compatible",
      capabilities: {
        streaming: true,
        tool_calls: true,
        vision: true,
        max_context_tokens: 128000,
      },
      endpoint_fields: [
        {
          id: "base_url",
          label: "Base URL",
          kind: "url",
          required: false,
          secret: false,
          validation: null,
          options: [],
          visible_when: [],
          help_text: "Defaults to the catalog endpoint",
        },
      ],
      model_source: "catalog",
      auth_options: [
        {
          id: "api_key",
          label: "API Key",
          auth_kind: "api_key",
          fields: [
            {
              id: "api_key",
              label: "API Key",
              kind: "secret",
              required: true,
              secret: true,
              validation: null,
              options: [],
              visible_when: [],
              help_text: null,
            },
          ],
          expiry_behavior: "never_expires",
          refresh_behavior: "not_refreshable",
          android_support: "fully_supported",
          wire_header: "authorization",
          wire_prefix: "Bearer ",
          extra_headers: [],
        },
      ],
      availability: "available",
      documentation_url: null,
      default_base_url: "https://api.openai.com/v1",
    },
  ],
};

function mockCatalog() {
  vi.mocked(getProviderCatalog).mockResolvedValue(catalog as never);
  vi.mocked(listProviderProfiles).mockResolvedValue([]);
}

it("renders catalog providers and saves a masked key through the profile command", async () => {
  mockCatalog();
  const act = vi.fn().mockResolvedValue(true);
  render(<Configuration snapshot={snapshot} view="settings" act={act} />);
  const field = await screen.findByLabelText("API Key");
  expect(field).toHaveAttribute("type", "password");
  fireEvent.change(field, { target: { value: "fixture-not-real-key" } });
  fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
  await waitFor(() =>
    expect(saveProviderProfile).toHaveBeenCalledWith({
      provider_id: "openai",
      auth_option_id: "api_key",
      values: { base_url: "https://api.openai.com/v1" },
      secrets: { api_key: "fixture-not-real-key" },
    }),
  );
  expect(await screen.findByText("Provider saved.")).toBeVisible();
});

it("keeps the entered key when the backend rejects the profile", async () => {
  mockCatalog();
  vi.mocked(saveProviderProfile).mockRejectedValue(new Error("rejected"));
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  const field = await screen.findByLabelText("API Key");
  fireEvent.change(field, { target: { value: "fixture-not-real-key" } });
  fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
  expect(await screen.findByRole("alert")).toBeVisible();
  expect(field).toHaveValue("fixture-not-real-key");
});

it("lists saved profiles with key state and removes them", async () => {
  mockCatalog();
  vi.mocked(listProviderProfiles).mockResolvedValue([
    {
      profile: {
        schema: "aethra.provider-profile.v1",
        provider_id: "openai",
        auth_option_id: "api_key",
        non_secret_config: {},
        enabled: true,
        display_name: "OpenAI",
        created_at: 1,
        updated_at: 1,
      },
      key_configured: true,
      active: true,
    },
  ]);
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  expect(await screen.findByText("Key stored")).toBeVisible();
  expect(screen.getByText("Active")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Remove OpenAI" }));
  fireEvent.click(
    screen.getByRole("button", { name: "Confirm removal of OpenAI" }),
  );
  await waitFor(() =>
    expect(deleteProviderProfile).toHaveBeenCalledWith("openai"),
  );
});

it("tests a saved profile connection and shows the typed outcome", async () => {
  mockCatalog();
  vi.mocked(listProviderProfiles).mockResolvedValue([
    {
      profile: {
        schema: "aethra.provider-profile.v1",
        provider_id: "openai",
        auth_option_id: "api_key",
        non_secret_config: {},
        enabled: true,
        display_name: "OpenAI",
        created_at: 1,
        updated_at: 1,
      },
      key_configured: true,
      active: false,
    },
  ]);
  vi.mocked(testProviderConnection).mockResolvedValue({
    success: false,
    failure_kind: "credential",
    model_id: null,
    latency_ms: null,
    message: "Authentication was rejected. Check the API key.",
  });
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  expect(await screen.findByText("Key stored")).toBeVisible();
  fireEvent.click(
    screen.getByRole("button", { name: "Test OpenAI connection" }),
  );
  await waitFor(() =>
    expect(testProviderConnection).toHaveBeenCalledWith("openai"),
  );
  expect(
    await screen.findByText("Authentication was rejected. Check the API key."),
  ).toBeVisible();
});

it("marks a profile active for inference", async () => {
  mockCatalog();
  vi.mocked(listProviderProfiles).mockResolvedValue([
    {
      profile: {
        schema: "aethra.provider-profile.v1",
        provider_id: "openai",
        auth_option_id: "api_key",
        non_secret_config: {},
        enabled: true,
        display_name: "OpenAI",
        created_at: 1,
        updated_at: 1,
      },
      key_configured: true,
      active: false,
    },
  ]);
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  expect(await screen.findByText("Key stored")).toBeVisible();
  fireEvent.click(
    screen.getByRole("button", { name: "Use OpenAI for inference" }),
  );
  await waitFor(() => expect(setActiveProvider).toHaveBeenCalledWith("openai"));
  expect(await screen.findByText("Active provider updated.")).toBeVisible();
});

it("loads an inactive profile for editing without replacing the active one", async () => {
  mockCatalog();
  vi.mocked(listProviderProfiles).mockResolvedValue([
    {
      profile: {
        schema: "aethra.provider-profile.v1",
        provider_id: "openai",
        auth_option_id: "api_key",
        non_secret_config: { base_url: "https://saved.example.com/v1" },
        enabled: true,
        display_name: "OpenAI",
        created_at: 1,
        updated_at: 1,
      },
      key_configured: true,
      active: false,
    },
  ]);
  render(<Configuration snapshot={snapshot} view="settings" act={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Edit OpenAI" }));
  expect(await screen.findByLabelText("Base URL")).toHaveValue(
    "https://saved.example.com/v1",
  );
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
    if (name === "authorize_connection")
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
  expect(command).toHaveBeenCalledWith("authorize_connection", {
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

it("completes native authorization without exposing a provider URL", async () => {
  vi.mocked(command).mockImplementation(async (name) => {
    if (name === "authorize_connection")
      return {
        transaction_id: "transaction",
        connection_id: "connection",
        expires_at: 123,
        requested_scopes: ["read"],
        resume_task_id: null,
      };
    if (name === "poll_authorization") return { state: "connected" };
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
  fireEvent.change(screen.getByLabelText("Redirect URI"), {
    target: { value: "https://app.example.com/oauth/callback" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  expect(
    await screen.findByText("Connected. Resuming your task."),
  ).toBeVisible();
  expect(
    screen.queryByRole("link", { name: "Open secure sign-in" }),
  ).not.toBeInTheDocument();
  expect(command).toHaveBeenCalledWith("poll_authorization", {
    transaction_id: "transaction",
  });
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
  fireEvent.click(
    screen.getByText("Optional offline conversation model", {
      selector: "summary",
    }),
  );
  expect(await screen.findByLabelText("Model download progress")).toHaveValue(
    50,
  );
  expect(screen.getByText("1.0 MB of 2.0 MB downloaded (50%)")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Cancel setup" }));
  await waitFor(() =>
    expect(command).toHaveBeenCalledWith("cancel_model_download", {}),
  );
});

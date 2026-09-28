import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ProviderCatalogSettings } from "./ProviderCatalogSettings";
import type { ProviderCatalogPayload } from "./types";

const catalog: ProviderCatalogPayload = {
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
          help_text: "Defaults to https://api.openai.com/v1",
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
      documentation_url: "https://platform.openai.com/docs",
      default_base_url: "https://api.openai.com/v1",
    },
    {
      schema: "aethra.provider-catalog.v1",
      id: "aws-bedrock",
      display_name: "AWS Bedrock",
      transport_family: "aws_bedrock",
      capabilities: {
        streaming: true,
        tool_calls: true,
        vision: true,
        max_context_tokens: null,
      },
      endpoint_fields: [
        {
          id: "region",
          label: "AWS Region",
          kind: "text",
          required: true,
          secret: false,
          validation: null,
          options: [],
          visible_when: [],
          help_text: null,
        },
      ],
      model_source: "user_specified",
      auth_options: [
        {
          id: "sigv4",
          label: "AWS Credentials (SigV4)",
          auth_kind: "cloud_identity",
          fields: [],
          expiry_behavior: "never_expires",
          refresh_behavior: "manual_reconnect",
          android_support: "partially_supported",
          wire_header: null,
          wire_prefix: null,
          extra_headers: [],
        },
      ],
      availability: "disabled_by_feature",
      documentation_url: null,
      default_base_url: null,
    },
  ],
};

const multiAuthCatalog: ProviderCatalogPayload = {
  ...catalog,
  providers: [
    {
      ...catalog.providers[0],
      auth_options: [
        ...catalog.providers[0].auth_options,
        {
          id: "bearer",
          label: "Bearer Token",
          auth_kind: "bearer_token",
          fields: [
            {
              id: "bearer_token",
              label: "Bearer Token",
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
    },
    ...catalog.providers.slice(1),
  ],
};

it("lists providers and renders the selected descriptor fields", async () => {
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      onSave={async () => true}
    />,
  );
  await waitFor(() => expect(screen.getByText("OpenAI")).toBeDefined());
  expect(screen.getByLabelText("Base URL")).toBeDefined();
  expect(screen.getByLabelText("API Key")).toHaveProperty("type", "password");
});

it("marks unavailable providers and switches descriptors", async () => {
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      onSave={async () => true}
    />,
  );
  await waitFor(() => expect(screen.getByText("OpenAI")).toBeDefined());
  const select = screen.getByLabelText("Provider") as HTMLSelectElement;
  expect(select.options[1]?.disabled).toBe(true);
  fireEvent.change(select, { target: { value: "aws-bedrock" } });
  expect(screen.getByLabelText("AWS Region")).toBeDefined();
});

it("emits a draft with secrets kept out of rendered values", async () => {
  const onSave = vi.fn().mockResolvedValue(true);
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      onSave={onSave}
    />,
  );
  await waitFor(() => expect(screen.getByText("OpenAI")).toBeDefined());
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "sk-test" },
  });
  fireEvent.click(screen.getByText("Save provider"));
  expect(onSave).toHaveBeenCalledWith({
    provider_id: "openai",
    auth_option_id: "api_key",
    values: { base_url: "https://api.openai.com/v1" },
    secrets: { api_key: "sk-test" },
  });
});

it("prefills the base URL with the catalog default but keeps it editable", async () => {
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      onSave={async () => true}
    />,
  );
  const field = (await screen.findByLabelText("Base URL")) as HTMLInputElement;
  expect(field.value).toBe("https://api.openai.com/v1");
  fireEvent.change(field, {
    target: { value: "https://proxy.example.com/v1" },
  });
  expect(field.value).toBe("https://proxy.example.com/v1");
});

it("restores an active profile's non-secret values without exposing its key", async () => {
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      initialProfile={{
        schema: "aethra.provider-profile.v1",
        provider_id: "openai",
        auth_option_id: "api_key",
        non_secret_config: { base_url: "https://proxy.example.com/v1" },
        enabled: true,
        display_name: "OpenAI",
        created_at: 1,
        updated_at: 2,
      }}
      onSave={async () => true}
    />,
  );
  await waitFor(() =>
    expect(screen.getByLabelText("Base URL")).toHaveValue(
      "https://proxy.example.com/v1",
    ),
  );
  expect(screen.getByLabelText("API Key")).toHaveValue("");
  expect(screen.getByRole("button", { name: "Update provider" })).toBeVisible();
});

it("clears an incompatible secret when the auth option changes", async () => {
  const onSave = vi.fn().mockResolvedValue(true);
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(multiAuthCatalog)}
      onSave={onSave}
    />,
  );
  await screen.findByLabelText("API Key");
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "old-api-key" },
  });
  fireEvent.change(screen.getByLabelText("Authentication"), {
    target: { value: "bearer" },
  });
  expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Bearer Token"), {
    target: { value: "bearer-token" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
  expect(onSave).toHaveBeenCalledWith(
    expect.objectContaining({
      auth_option_id: "bearer",
      secrets: { bearer_token: "bearer-token" },
    }),
  );
});

it("updates non-secret fields without re-entering a stored secret", async () => {
  const onSave = vi.fn().mockResolvedValue(true);
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      initialProfile={{
        schema: "aethra.provider-profile.v1",
        provider_id: "openai",
        auth_option_id: "api_key",
        non_secret_config: { base_url: "https://proxy.example.com/v1" },
        enabled: true,
        display_name: "OpenAI",
        created_at: 1,
        updated_at: 2,
      }}
      hasStoredSecret
      onSave={onSave}
    />,
  );
  const key = await screen.findByLabelText("API Key");
  expect(key).not.toBeRequired();
  fireEvent.change(key, { target: { value: "" } });
  fireEvent.click(screen.getByRole("button", { name: "Update provider" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ secrets: {} }));
});

it("blocks an invalid required field before calling the backend", async () => {
  const onSave = vi.fn().mockResolvedValue(true);
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.resolve(catalog)}
      onSave={onSave}
    />,
  );
  await screen.findByLabelText("API Key");
  fireEvent.click(screen.getByRole("button", { name: "Save provider" }));
  expect(onSave).not.toHaveBeenCalled();
  expect(screen.getByText("API Key is required.")).toBeVisible();
});

it("shows a recoverable error when the catalog fails", async () => {
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.reject(new Error("offline"))}
      onSave={async () => true}
    />,
  );
  await waitFor(() => expect(screen.getByRole("alert")).toBeDefined());
});

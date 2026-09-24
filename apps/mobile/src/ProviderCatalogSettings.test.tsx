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

it("lists providers and renders the selected descriptor fields", async () => {
  render(
    <ProviderCatalogSettings loadCatalog={() => Promise.resolve(catalog)} onSave={() => {}} />,
  );
  await waitFor(() => expect(screen.getByText("OpenAI")).toBeDefined());
  expect(screen.getByLabelText("Base URL")).toBeDefined();
  expect(screen.getByLabelText("API Key")).toHaveProperty("type", "password");
});

it("marks unavailable providers and switches descriptors", async () => {
  render(
    <ProviderCatalogSettings loadCatalog={() => Promise.resolve(catalog)} onSave={() => {}} />,
  );
  await waitFor(() => expect(screen.getByText("OpenAI")).toBeDefined());
  const select = screen.getByLabelText("Provider") as HTMLSelectElement;
  expect(select.options[1]?.disabled).toBe(true);
  fireEvent.change(select, { target: { value: "aws-bedrock" } });
  expect(screen.getByLabelText("AWS Region")).toBeDefined();
});

it("emits a draft with secrets kept out of rendered values", async () => {
  const onSave = vi.fn();
  render(
    <ProviderCatalogSettings loadCatalog={() => Promise.resolve(catalog)} onSave={onSave} />,
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
    <ProviderCatalogSettings loadCatalog={() => Promise.resolve(catalog)} onSave={() => {}} />,
  );
  const field = (await screen.findByLabelText("Base URL")) as HTMLInputElement;
  expect(field.value).toBe("https://api.openai.com/v1");
  fireEvent.change(field, { target: { value: "https://proxy.example.com/v1" } });
  expect(field.value).toBe("https://proxy.example.com/v1");
});

it("shows a recoverable error when the catalog fails", async () => {
  render(
    <ProviderCatalogSettings
      loadCatalog={() => Promise.reject(new Error("offline"))}
      onSave={() => {}}
    />,
  );
  await waitFor(() => expect(screen.getByRole("alert")).toBeDefined());
});

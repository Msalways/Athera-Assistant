import { expect, it } from "vitest";
import type { ProviderCatalogPayload, ProviderDefinition } from "./types";

const fixture: ProviderCatalogPayload = {
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
      id: "openai-compatible",
      display_name: "OpenAI-Compatible (Custom)",
      transport_family: "open_ai_compatible",
      capabilities: {
        streaming: true,
        tool_calls: true,
        vision: false,
        max_context_tokens: null,
      },
      endpoint_fields: [
        {
          id: "base_url",
          label: "Base URL",
          kind: "url",
          required: true,
          secret: false,
          validation: null,
          options: [],
          visible_when: [],
          help_text: "Custom OpenAI-compatible endpoint",
        },
      ],
      model_source: "user_specified",
      auth_options: [
        {
          id: "none",
          label: "No Auth",
          auth_kind: "none",
          fields: [],
          expiry_behavior: "never_expires",
          refresh_behavior: "not_refreshable",
          android_support: "fully_supported",
          wire_header: null,
          wire_prefix: null,
          extra_headers: [],
        },
      ],
      availability: "available",
      documentation_url: null,
      default_base_url: null,
    },
  ],
};

function assertNoSecretValues(payload: ProviderCatalogPayload): void {
  const json = JSON.stringify(payload).toLowerCase();
  for (const marker of ['"sk-"', "bearer sk-"]) {
    expect(json).not.toContain(marker);
  }
  for (const provider of payload.providers) {
    for (const option of provider.auth_options) {
      for (const field of option.fields) {
        if (field.secret) {
          expect(field.id).not.toMatch(/sk-/i);
        }
      }
    }
  }
}

it("provider catalog snapshot round-trips", () => {
  expect(fixture.schema).toBe("aethra.provider-catalog-registry.v1");
  const decoded = JSON.parse(JSON.stringify(fixture)) as ProviderCatalogPayload;
  expect(decoded.providers.map((p) => p.id)).toEqual([
    "openai",
    "openai-compatible",
  ]);
  const openai = decoded.providers[0] as ProviderDefinition;
  expect(openai.transport_family).toBe("open_ai_compatible");
  expect(openai.auth_options[0]?.auth_kind).toBe("api_key");
  expect(openai.auth_options[0]?.wire_header).toBe("authorization");
  expect(openai.auth_options[0]?.wire_prefix).toBe("Bearer ");
});

it("provider catalog carries no secret values", () => {
  assertNoSecretValues(fixture);
});

it("custom provider requires its endpoint", () => {
  const custom = fixture.providers.find((p) => p.id === "openai-compatible");
  expect(custom).toBeDefined();
  const baseUrl = custom?.endpoint_fields.find((f) => f.id === "base_url");
  expect(baseUrl?.required).toBe(true);
});

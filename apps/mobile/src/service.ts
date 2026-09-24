import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  BuildInfo,
  ProviderCatalogPayload,
  ProviderProfile,
  ProviderProfileDraft,
  SavedProviderProfile,
} from "./types";

export async function command<T>(
  name: string,
  payload: unknown = {},
): Promise<T> {
  if (isTauri()) return invoke<T>("command", { name, payload });
  const response = await fetch("/api/command", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Assistant-Client": "local-ui",
    },
    body: JSON.stringify({ name, payload }),
  });
  const body = await response.json();
  if (!response.ok) throw new Error(body.error || "Request failed");
  return body as T;
}

export async function getBuildInfo(): Promise<BuildInfo | null> {
  if (isTauri()) {
    try {
      return await invoke<BuildInfo>("build_info");
    } catch {
      return null;
    }
  }
  return null;
}

export async function getProviderCatalog(): Promise<ProviderCatalogPayload> {
  return command<ProviderCatalogPayload>("get_provider_catalog", {});
}

export async function saveProviderProfile(
  draft: ProviderProfileDraft,
): Promise<ProviderProfile> {
  const entered = Object.values(draft.secrets).filter((v) => v !== "");
  const secret =
    entered.length === 0
      ? null
      : entered.length === 1
        ? entered[0]
        : JSON.stringify(draft.secrets);
  return command<ProviderProfile>("save_provider_profile", {
    provider_id: draft.provider_id,
    auth_option_id: draft.auth_option_id,
    config: draft.values,
    secret,
  });
}

export async function listProviderProfiles(): Promise<SavedProviderProfile[]> {
  return command<SavedProviderProfile[]>("list_provider_profiles", {});
}

export async function deleteProviderProfile(
  provider_id: string,
): Promise<void> {
  await command("delete_provider_profile", { provider_id });
}

export async function setActiveProvider(provider_id: string): Promise<void> {
  await command("set_active_provider", { provider_id });
}

export interface ConnectionTestOutcome {
  success: boolean;
  failure_kind: string | null;
  model_id: string | null;
  latency_ms: number | null;
  message: string;
}

export async function testProviderConnection(
  provider_id: string,
): Promise<ConnectionTestOutcome> {
  return command<ConnectionTestOutcome>("test_provider_connection", {
    provider_id,
  });
}

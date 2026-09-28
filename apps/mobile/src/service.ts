import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  BuildInfo,
  FailoverPolicy,
  ProviderCatalogPayload,
  ProviderProfile,
  ProviderProfileDraft,
  SavedProviderProfile,
} from "./types";

export interface CommandOptions {
  timeoutMs?: number;
}

export class CommandTimeoutError extends Error {
  constructor(commandName: string, timeoutMs: number) {
    super(
      `The ${commandName} request did not respond within ${Math.round(timeoutMs / 1000)} seconds.`,
    );
    this.name = "CommandTimeoutError";
  }
}

function withTimeout<T>(
  promise: Promise<T>,
  commandName: string,
  timeoutMs: number,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new CommandTimeoutError(commandName, timeoutMs)),
      timeoutMs,
    );
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

export async function command<T>(
  name: string,
  payload: unknown = {},
  options: CommandOptions = {},
): Promise<T> {
  const timeoutMs = options.timeoutMs ?? 15000;
  if (isTauri()) {
    return withTimeout(
      invoke<T>("command", { name, payload }),
      name,
      timeoutMs,
    );
  }
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);
  try {
    const response = await fetch("/api/command", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Assistant-Client": "local-ui",
      },
      body: JSON.stringify({ name, payload }),
      signal: controller.signal,
    });
    const body = await response.json();
    if (!response.ok) throw new Error(body.error || "Request failed");
    return body as T;
  } catch (error) {
    if (timedOut) throw new CommandTimeoutError(name, timeoutMs);
    throw error;
  } finally {
    clearTimeout(timer);
  }
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

export async function setFailoverPolicy(
  policy: FailoverPolicy,
): Promise<boolean> {
  return command<boolean>("set_failover_policy", policy, { timeoutMs: 30000 });
}

export async function clearFailoverPolicy(): Promise<boolean> {
  return command<boolean>("clear_failover_policy", {}, { timeoutMs: 30000 });
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

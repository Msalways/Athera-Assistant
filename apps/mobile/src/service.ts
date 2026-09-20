import { invoke, isTauri } from "@tauri-apps/api/core";
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

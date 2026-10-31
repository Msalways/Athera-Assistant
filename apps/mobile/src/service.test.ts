import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  CommandTimeoutError,
  command,
  testProviderConnection,
} from "./service";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => true,
}));

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

it("turns a pending Tauri command into a bounded timeout", async () => {
  vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

  const pending = command("snapshot", {}, { timeoutMs: 2000 });
  const assertion = expect(pending).rejects.toEqual(
    expect.objectContaining<Partial<CommandTimeoutError>>({
      name: "CommandTimeoutError",
      message: "The snapshot request did not respond within 2 seconds.",
    }),
  );

  await vi.advanceTimersByTimeAsync(2000);

  await assertion;
});

it("waits past the default UI deadline for a provider connection check", async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const pending = testProviderConnection("nvidia-nim");
  let settled = false;
  void pending.finally(() => {
    settled = true;
  });

  await vi.advanceTimersByTimeAsync(16000);
  expect(settled).toBe(false);

  const result = {
    success: true,
    failure_kind: null,
    model_id: "z-ai/glm-5.3-flash",
    latency_ms: 16000,
    message: "Connected",
  };
  resolve(result);
  await expect(pending).resolves.toEqual(result);
});

import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CommandTimeoutError, command } from "./service";

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

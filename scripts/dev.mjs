import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, delimiter, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

const root = fileURLToPath(new URL("../", import.meta.url));
const healthUrl = "http://127.0.0.1:8787/api/health";

export async function backendReady(url = healthUrl) {
  let response;
  try {
    response = await fetch(url, { signal: AbortSignal.timeout(1000) });
  } catch (error) {
    if (
      ["ECONNREFUSED", "ECONNRESET", "UND_ERR_SOCKET"].includes(
        error.cause?.code,
      )
    ) {
      return false;
    }
    throw new Error(`Cannot check the Rust backend at ${url}`, {
      cause: error,
    });
  }
  const body = await response.json().catch(() => null);
  if (
    !response.ok ||
    body?.service !== "assistant-dev" ||
    body.status !== "ready"
  ) {
    throw new Error(
      "Port 8787 is occupied by an incompatible service. Stop the old backend or free that port, then retry.",
    );
  }
  return true;
}

export async function waitForBackend({
  url = healthUrl,
  timeoutMs = 300000,
  signal,
} = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    signal?.throwIfAborted();
    if (await backendReady(url)) return;
    await delay(200, undefined, { signal });
  }
  throw new Error(
    "Rust backend did not become ready within the startup timeout.",
  );
}

function developmentEnvironment() {
  const env = { ...process.env };
  const localCargo = join(root, ".tools", "cargo");
  const cargoExe = process.platform === "win32" ? "cargo.exe" : "cargo";
  if (existsSync(join(localCargo, "bin", cargoExe))) {
    env.CARGO_HOME ??= localCargo;
    env.RUSTUP_HOME ??= join(root, ".tools", "rustup");
  }
  // Windows environment names are case-insensitive; avoid duplicate Path/PATH keys.
  const pathKey = Object.keys(env).find((key) => key.toUpperCase() === "PATH");
  const inheritedPath = env[pathKey] ?? "";
  if (pathKey) delete env[pathKey];
  env.PATH = [
    dirname(process.execPath),
    ...(env.CARGO_HOME ? [join(env.CARGO_HOME, "bin")] : []),
    inheritedPath,
  ].join(delimiter);
  const needle = join(
    root,
    ".tools",
    "needle2",
    "windows",
    "needle",
    "libneedle.dll",
  );
  if (process.platform === "win32" && existsSync(needle)) {
    env.ASSISTANT_NEEDLE_LIBRARY ??= needle;
  }
  const localChat = join(
    root,
    "vendor",
    "local-chat",
    "build-host",
    "Release",
    "athera-chat.exe",
  );
  if (process.platform === "win32" && existsSync(localChat)) {
    env.ASSISTANT_LLAMA_CLI ??= localChat;
  }
  return env;
}

async function main() {
  let backend;
  let server;
  let stopping = false;
  const startup = new AbortController();
  async function stop(code) {
    if (stopping) return;
    stopping = true;
    process.exitCode = code;
    startup.abort();
    if (
      backend?.pid &&
      backend.exitCode === null &&
      backend.signalCode === null
    ) {
      // Cargo owns the bridge child too. Only terminate the process tree we started.
      if (process.platform === "win32") {
        const result = spawnSync(
          "taskkill",
          ["/PID", String(backend.pid), "/T", "/F"],
          {
            windowsHide: true,
            stdio: "ignore",
          },
        );
        if (result.error || result.status !== 0) {
          console.error(
            `Could not stop the owned Rust process tree (PID ${backend.pid}). Check process permissions.`,
          );
          process.exitCode = 1;
        }
      } else {
        try {
          process.kill(-backend.pid, "SIGTERM");
        } catch (error) {
          if (error.code !== "ESRCH") throw error;
        }
      }
    }
    await server?.close();
  }
  process.once("SIGINT", () => void stop(0));
  process.once("SIGTERM", () => void stop(0));
  try {
    const ready = await backendReady();
    startup.signal.throwIfAborted();
    if (ready) {
      console.log(
        "Using the Rust development bridge already running on port 8787.",
      );
    } else {
      console.log(
        "Building and starting the Rust backend. First startup may take several minutes.",
      );
      backend = spawn(
        "cargo",
        ["run", "-p", "assistant-cli", "--bin", "assistant-dev"],
        {
          cwd: root,
          env: developmentEnvironment(),
          stdio: "inherit",
          windowsHide: true,
          detached: process.platform !== "win32",
        },
      );
      backend.once("error", () => {
        console.error(
          "Could not start Cargo. Install Rust or run scripts/bootstrap.ps1.",
        );
        void stop(1);
      });
      backend.once("exit", (code) => {
        if (!stopping) {
          console.error(
            `Rust backend exited (${code ?? "signal"}). Stopping the frontend.`,
          );
          void stop(1);
        }
      });
      await waitForBackend({ signal: startup.signal });
    }
    startup.signal.throwIfAborted();
    process.chdir(join(root, "apps", "mobile"));
    const { createServer } = await import("vite");
    server = await createServer({ server: { host: "127.0.0.1" } });
    if (stopping) {
      await server.close();
      return;
    }
    await server.listen();
    if (stopping) {
      await server.close();
      return;
    }
    server.printUrls();
    console.log("Press Ctrl+C to stop this development session.");
  } catch (error) {
    if (!stopping) console.error(error.message);
    await stop(stopping ? process.exitCode : 1);
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await main();
}

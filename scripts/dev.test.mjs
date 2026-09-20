import assert from "node:assert/strict";
import { createServer } from "node:http";
import { once } from "node:events";
import test from "node:test";
import { backendReady, waitForBackend } from "./dev.mjs";

test("startup waits for the correct bridge and rejects an unrelated service", async (t) => {
  let healthy = true;
  const server = createServer((request, response) => {
    assert.equal(request.url, "/api/health");
    response.setHeader("Content-Type", "application/json");
    response.end(
      JSON.stringify(
        healthy
          ? { service: "assistant-dev", status: "ready" }
          : { service: "unrelated" },
      ),
    );
  });
  t.after(() => server.close());
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = server.address().port;
  const url = `http://127.0.0.1:${port}/api/health`;
  assert.equal(await backendReady(url), true);
  await waitForBackend({ url, timeoutMs: 1000 });
  healthy = false;
  await assert.rejects(backendReady(url), /incompatible service/);
  await new Promise((resolve) => server.close(resolve));
  assert.equal(await backendReady(url), false);
  await assert.rejects(
    waitForBackend({ url, timeoutMs: 50 }),
    /startup timeout/,
  );
  await assert.rejects(
    waitForBackend({ url, signal: AbortSignal.abort() }),
    /abort/i,
  );
  const waiting = waitForBackend({ url, timeoutMs: 2000 });
  server.listen(port, "127.0.0.1");
  healthy = true;
  await waiting;
});

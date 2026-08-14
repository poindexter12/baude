#!/usr/bin/env node
// Spike 001a — opencode remote permission approval via server API.
//
// Given `permission: { bash: "ask" }`, when the agent requests a bash tool
// call, then `permission.updated` arrives on the /event SSE stream and
// POST /session/{id}/permissions/{permissionID} {response: once|reject}
// resolves it. Approve path must produce the proof file; reject path must
// provably NOT produce it.
//
// Forensic log: every SSE event + every action is appended to events.jsonl
// with ISO timestamps; a summary JSON is written to result.json.

import { spawn } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync, appendFileSync, existsSync, readFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SANDBOX = path.join(HERE, "sandbox");
const LOG = path.join(HERE, "events.jsonl");
const PORT = 14711;
const BASE = `http://127.0.0.1:${PORT}`;

const events = [];
function log(category, data) {
  const entry = { ts: new Date().toISOString(), category, ...data };
  events.push(entry);
  appendFileSync(LOG, JSON.stringify(entry) + "\n");
  console.log(`[${entry.ts}] ${category}${data.type ? " " + data.type : ""}${data.note ? " — " + data.note : ""}`);
}

// --- sandbox: a fresh opencode project with bash gated behind "ask" ---
rmSync(SANDBOX, { recursive: true, force: true });
rmSync(LOG, { force: true });
mkdirSync(SANDBOX, { recursive: true });
writeFileSync(
  path.join(SANDBOX, "opencode.json"),
  JSON.stringify({ $schema: "https://opencode.ai/config.json", permission: { bash: "ask" } }, null, 2)
);

// --- server ---
const serve = spawn("opencode", ["serve", "--port", String(PORT), "--hostname", "127.0.0.1"], {
  cwd: SANDBOX,
  stdio: ["ignore", "pipe", "pipe"],
});
serve.stdout.on("data", (d) => appendFileSync(path.join(HERE, "serve.log"), d));
serve.stderr.on("data", (d) => appendFileSync(path.join(HERE, "serve.log"), d));
const cleanup = () => { try { serve.kill(); } catch {} };
process.on("exit", cleanup);

async function waitForServer() {
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(`${BASE}/global/health`);
      if (r.ok) return;
    } catch {}
    await sleep(500);
  }
  throw new Error("server never became healthy");
}

// --- SSE watcher: collect events, let tests await a predicate ---
const waiters = [];
function waitForEvent(predicate, label, timeoutMs = 180_000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${label}`)), timeoutMs);
    waiters.push({ predicate, resolve: (e) => { clearTimeout(timer); resolve(e); } });
  });
}
async function startSse() {
  const res = await fetch(`${BASE}/event`);
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  (async () => {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let idx;
      while ((idx = buf.indexOf("\n\n")) >= 0) {
        const chunk = buf.slice(0, idx);
        buf = buf.slice(idx + 2);
        for (const line of chunk.split("\n")) {
          if (!line.startsWith("data:")) continue;
          let ev;
          try { ev = JSON.parse(line.slice(5).trim()); } catch { continue; }
          log("sse", { type: ev.type, properties: ev.properties });
          for (let i = waiters.length - 1; i >= 0; i--) {
            if (waiters[i].predicate(ev)) waiters.splice(i, 1)[0].resolve(ev);
          }
        }
      }
    }
  })().catch((e) => log("sse", { note: `stream ended: ${e.message}` }));
}

async function api(method, url, body) {
  const res = await fetch(`${BASE}${url}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let json; try { json = JSON.parse(text); } catch { json = text; }
  log("api", { note: `${method} ${url} -> ${res.status}`, response: json });
  if (!res.ok) throw new Error(`${method} ${url} failed: ${res.status} ${text}`);
  return json;
}

// One approval round-trip: prompt for a bash command, wait for the pending
// permission, hold it `holdMs` to prove it is genuinely deferred, then reply.
async function round({ name, proofFile, response }) {
  const session = await api("POST", "/session", { title: `spike-001a-${name}` });
  const sid = session.id;

  // opencode 1.18.16 emits "permission.asked"; the bundled SDK types still say
  // "permission.updated" — accept both (schema churn finding, see README).
  const permP = waitForEvent(
    (e) => (e.type === "permission.asked" || e.type === "permission.updated") && e.properties.sessionID === sid,
    `permission.asked (${name})`
  );
  await api("POST", `/session/${sid}/prompt_async`, {
    parts: [{
      type: "text",
      text: `Use the bash tool to run exactly this command and nothing else: echo ${name}-proof > ${proofFile}. Do not use any other tool. After the command, just say done.`,
    }],
  });

  const perm = (await permP).properties;
  log("test", { note: `${name}: permission pending — id=${perm.id} permission=${perm.permission ?? perm.type} patterns=${JSON.stringify(perm.patterns ?? perm.pattern)}` });

  // Hold the decision to prove the agent is blocked awaiting a remote reply.
  const holdMs = 3000;
  await sleep(holdMs);
  const ranEarly = existsSync(path.join(SANDBOX, proofFile));
  log("test", { note: `${name}: after ${holdMs}ms hold, proof file exists=${ranEarly} (must be false while pending)` });

  const repliedP = waitForEvent(
    // live server: {sessionID, requestID, reply}; SDK types claim {permissionID, response}
    (e) => e.type === "permission.replied" && (e.properties.requestID === perm.id || e.properties.permissionID === perm.id),
    `permission.replied (${name})`
  );
  const idleP = waitForEvent(
    (e) => e.type === "session.idle" && e.properties.sessionID === sid,
    `session.idle (${name})`
  );
  const ok = await api("POST", `/session/${sid}/permissions/${perm.id}`, { response });
  const replied = await repliedP;
  await idleP;

  const proofPath = path.join(SANDBOX, proofFile);
  const proofExists = existsSync(proofPath);
  return {
    name,
    response,
    replyAccepted: ok === true,
    heldWithoutExecuting: !ranEarly,
    repliedEvent: replied.properties,
    proofExists,
    proofContent: proofExists ? readFileSync(proofPath, "utf8").trim() : null,
    permission: { id: perm.id, permission: perm.permission ?? perm.type, patterns: perm.patterns ?? perm.pattern, always: perm.always, metadata: perm.metadata, tool: perm.tool },
  };
}

// --- run ---
try {
  log("test", { note: "starting opencode serve" });
  await waitForServer();
  log("test", { note: "server healthy" });
  await startSse();

  const approve = await round({ name: "approve", proofFile: "proof-approve.txt", response: "once" });
  const reject = await round({ name: "reject", proofFile: "proof-reject.txt", response: "reject" });

  const verdictOk =
    approve.replyAccepted && approve.heldWithoutExecuting && approve.proofExists &&
    reject.replyAccepted && reject.heldWithoutExecuting && !reject.proofExists;

  const result = {
    verdict: verdictOk ? "VALIDATED" : "INVALIDATED",
    approve,
    reject,
    eventCounts: events.reduce((m, e) => ((m[e.category + ":" + (e.type ?? "-")] = (m[e.category + ":" + (e.type ?? "-")] ?? 0) + 1), m), {}),
  };
  writeFileSync(path.join(HERE, "result.json"), JSON.stringify(result, null, 2));
  console.log("\n=== RESULT ===");
  console.log(JSON.stringify(result, null, 2));
  process.exit(verdictOk ? 0 : 1);
} catch (e) {
  log("test", { note: `FATAL: ${e.message}` });
  writeFileSync(path.join(HERE, "result.json"), JSON.stringify({ verdict: "ERROR", error: e.message }, null, 2));
  process.exit(2);
} finally {
  cleanup();
}

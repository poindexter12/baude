#!/usr/bin/env node
// Spike 001b — opencode remote permission approval via a plugin
// `permission.ask` hook whose decision is DEFERRED (arrives seconds later
// from outside the process). Validates: hook fires, opencode awaits the
// async hook, and a late "allow"/"deny" is honored.
//
// Also records whether `permission.asked` still appears on the SSE stream
// when a plugin hook is registered (does the hook pre-empt the ask?).

import { spawn } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync, appendFileSync, existsSync, readFileSync, copyFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SANDBOX = path.join(HERE, "sandbox");
const LOG = path.join(HERE, "events.jsonl");
const PORT = 14712;
const BASE = `http://127.0.0.1:${PORT}`;

const events = [];
function log(category, data) {
  const entry = { ts: new Date().toISOString(), category, ...data };
  events.push(entry);
  appendFileSync(LOG, JSON.stringify(entry) + "\n");
  console.log(`[${entry.ts}] ${category}${data.type ? " " + data.type : ""}${data.note ? " — " + data.note : ""}`);
}

// --- sandbox: bash gated behind "ask", plugin installed in .opencode/plugins/ ---
rmSync(SANDBOX, { recursive: true, force: true });
rmSync(LOG, { force: true });
mkdirSync(path.join(SANDBOX, ".opencode", "plugins"), { recursive: true });
writeFileSync(
  path.join(SANDBOX, "opencode.json"),
  JSON.stringify({ $schema: "https://opencode.ai/config.json", permission: { bash: "ask" } }, null, 2)
);
copyFileSync(
  path.join(HERE, "plugin", "deferred-permission.js"),
  path.join(SANDBOX, ".opencode", "plugins", "deferred-permission.js")
);

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
    try { if ((await fetch(`${BASE}/global/health`)).ok) return; } catch {}
    await sleep(500);
  }
  throw new Error("server never became healthy");
}

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
  log("api", { note: `${method} ${url} -> ${res.status}` });
  if (!res.ok) throw new Error(`${method} ${url} failed: ${res.status} ${text}`);
  return json;
}

function pluginLogEntries() {
  const p = path.join(SANDBOX, "plugin-log.jsonl");
  if (!existsSync(p)) return [];
  return readFileSync(p, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
}
async function waitForPluginLog(predicate, label, timeoutMs = 120_000) {
  const start = Date.now();
  for (;;) {
    const hit = pluginLogEntries().find(predicate);
    if (hit) return hit;
    if (Date.now() - start > timeoutMs) throw new Error(`timeout waiting for plugin log: ${label}`);
    await sleep(300);
  }
}

// One round: prompt for a bash command, observe WHICH plugin surfaces see the
// pending permission, then resolve it — via the plugin's deferred decision if
// the permission.ask hook fired, else via the HTTP reply endpoint (fallback,
// recorded as such).
async function round({ name, proofFile, status }) {
  const session = await api("POST", "/session", { title: `spike-001b-${name}` });
  const sid = session.id;
  const askHookSeen = () => pluginLogEntries().filter((e) => e.note === "permission.ask fired").length;
  const eventHookSeen = () =>
    pluginLogEntries().filter((e) => e.note === "event hook received" && e.type === "permission.asked").length;
  const priorAskHook = askHookSeen();
  const priorEventHook = eventHookSeen();

  const idleP = waitForEvent(
    (e) => e.type === "session.idle" && e.properties.sessionID === sid,
    `session.idle (${name})`
  );
  const ssePermP = waitForEvent(
    (e) => e.type === "permission.asked" && e.properties.sessionID === sid,
    `sse permission.asked (${name})`
  );
  await api("POST", `/session/${sid}/prompt_async`, {
    parts: [{
      type: "text",
      text: `Use the bash tool to run exactly this command and nothing else: echo ${name}-proof > ${proofFile}. Do not use any other tool. After the command, just say done.`,
    }],
  });

  const perm = (await ssePermP).properties;
  // Give the plugin surfaces a moment to log, and hold to prove deferral.
  await sleep(3000);
  const askHookFired = askHookSeen() > priorAskHook;
  const eventHookFired = eventHookSeen() > priorEventHook;
  const ranEarly = existsSync(path.join(SANDBOX, proofFile));
  log("test", { note: `${name}: askHookFired=${askHookFired} eventHookFired=${eventHookFired} proofEarly=${ranEarly}` });

  let resolvedVia;
  if (askHookFired) {
    writeFileSync(path.join(SANDBOX, "decision.json"), JSON.stringify({ status }));
    resolvedVia = "plugin-deferred-decision";
  } else {
    // Hook is dead on this build — resolve via the HTTP endpoint so the run
    // completes; the finding stands on askHookFired/eventHookFired.
    await api("POST", `/session/${sid}/permissions/${perm.id}`, {
      response: status === "allow" ? "once" : "reject",
    });
    resolvedVia = "http-reply-endpoint-fallback";
  }
  log("test", { note: `${name}: resolved via ${resolvedVia}` });
  await idleP;

  const proofPath = path.join(SANDBOX, proofFile);
  const proofExists = existsSync(proofPath);
  const applied = pluginLogEntries().filter((e) => e.note === "deferred decision applied").pop();
  return {
    name,
    status,
    askHookFired,
    eventHookFired,
    resolvedVia,
    heldWithoutExecuting: !ranEarly,
    decisionApplied: applied ?? null,
    proofExists,
    proofContent: proofExists ? readFileSync(proofPath, "utf8").trim() : null,
  };
}

try {
  log("test", { note: "starting opencode serve (plugin sandbox)" });
  await waitForServer();
  log("test", { note: "server healthy" });
  await startSse();

  // Give plugin load a moment, then confirm it loaded at all.
  await waitForPluginLog((e) => e.note === "plugin loaded", "plugin loaded", 30_000);
  log("test", { note: "plugin loaded confirmed" });

  const allow = await round({ name: "allow", proofFile: "proof-allow.txt", status: "allow" });
  const deny = await round({ name: "deny", proofFile: "proof-deny.txt", status: "deny" });

  // VALIDATED only if the plugin's own deferred decision drove both rounds.
  const hookWorked =
    allow.askHookFired && allow.resolvedVia === "plugin-deferred-decision" &&
    deny.askHookFired && deny.resolvedVia === "plugin-deferred-decision";
  const behaviorOk =
    allow.heldWithoutExecuting && allow.proofExists &&
    deny.heldWithoutExecuting && !deny.proofExists;

  const result = {
    verdict: hookWorked && behaviorOk ? "VALIDATED" : "INVALIDATED",
    allow,
    deny,
    note: "askHookFired = documented permission.ask plugin hook; eventHookFired = generic event hook receiving permission.asked",
  };
  writeFileSync(path.join(HERE, "result.json"), JSON.stringify(result, null, 2));
  console.log("\n=== RESULT ===");
  console.log(JSON.stringify(result, null, 2));
  process.exit(hookWorked && behaviorOk ? 0 : 1);
} catch (e) {
  log("test", { note: `FATAL: ${e.message}` });
  writeFileSync(path.join(HERE, "result.json"), JSON.stringify({ verdict: "ERROR", error: e.message }, null, 2));
  process.exit(2);
} finally {
  cleanup();
}

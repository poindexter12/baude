// Spike 001b — probes which plugin surfaces see a pending permission on
// opencode 1.18.16:
//  - "permission.ask" hook (the documented decide-in-process hook)
//  - generic "event" hook (does permission.asked arrive as a bus event?)
// Both log to plugin-log.jsonl; the driver script watches that file.
import fs from "node:fs";
import path from "node:path";

export const DeferredPermission = async ({ directory }) => {
  const logFile = path.join(directory, "plugin-log.jsonl");
  const log = (o) => fs.appendFileSync(logFile, JSON.stringify({ ts: new Date().toISOString(), ...o }) + "\n");
  log({ note: "plugin loaded", directory });
  return {
    "permission.ask": async (input, output) => {
      log({ note: "permission.ask fired", input });
      const decisionFile = path.join(directory, "decision.json");
      for (let i = 0; i < 120; i++) {
        if (fs.existsSync(decisionFile)) {
          const d = JSON.parse(fs.readFileSync(decisionFile, "utf8"));
          fs.unlinkSync(decisionFile);
          output.status = d.status; // "allow" | "deny" | "ask"
          log({ note: "deferred decision applied", status: d.status, waitedMs: i * 500 });
          return;
        }
        await new Promise((r) => setTimeout(r, 500));
      }
      log({ note: "no decision within 60s — leaving status as default (ask)" });
    },
    event: async ({ event }) => {
      if (event.type && event.type.startsWith("permission.")) {
        log({ note: "event hook received", type: event.type, properties: event.properties });
      }
    },
  };
};

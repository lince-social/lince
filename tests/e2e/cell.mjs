//! Booting a Cell with the Organ Sand on its board, shared by the e2e specs.
//!
//! Extracted so a second spec can reuse it rather than copy it: the seeding
//! step has to match the real package catalogue, and two drifting copies of
//! that would test two different boards.

import { spawn } from "node:child_process";
import { closeSync, openSync } from "node:fs";
import net from "node:net";
import path from "node:path";

export const ROOT = path.resolve(import.meta.dirname, "../..");
export const BIN = path.join(ROOT, "target/debug/lince");

export async function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
  });
}

export async function waitForJson(url, timeout = 20_000) {
  const started = Date.now();
  let lastError = null;
  while (Date.now() - started < timeout) {
    try {
      const response = await fetch(url);
      if (response.ok) return response.json();
      lastError = new Error(`${response.status} ${await response.text()}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`Timed out waiting for ${url}: ${lastError}`);
}

export async function fetchJson(url, options = {}) {
  const response = await fetch(url, options);
  const text = await response.text();
  let payload = null;
  try { payload = text ? JSON.parse(text) : null; } catch { payload = text; }
  if (!response.ok) throw new Error(`${options.method || "GET"} ${url}: ${response.status} ${text}`);
  return payload;
}

export async function seedOrganSand(baseUrl) {
  const packages = await fetchJson(`${baseUrl}/host/packages/local`);
  const summary = packages.find((entry) => entry.title === "Organ");
  if (!summary) throw new Error("The official Organ Sand is absent from the local catalog");
  const preview = await fetchJson(`${baseUrl}/host/packages/local/${encodeURIComponent(summary.id)}`);
  const board = await fetchJson(`${baseUrl}/host/board/state`);
  const workspace = board.workspaces.find((entry) => entry.id === board.activeWorkspaceId);
  workspace.cards = workspace.cards.filter((card) => card.id !== "e2e-organ");
  workspace.cards.push({
    id: "e2e-organ",
    kind: "package",
    title: preview.title,
    description: preview.description,
    text: "",
    html: preview.html,
    author: preview.author,
    permissions: preview.permissions,
    packageName: preview.filename,
    requiresServer: preview.requires_server,
    serverId: "",
    streamsEnabled: true,
    widgetState: {},
    x: 4_100,
    y: 4_520,
    width: 760,
    height: 760,
    pinned: false,
    system: false,
    zIndex: 200,
    groupId: null,
    groupIds: [],
    abiListen: [],
  });
  await fetchJson(`${baseUrl}/host/board/state`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(board),
  });
}

export async function stopProcess(child) {
  if (!child || child.exitCode != null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    new Promise((resolve) => setTimeout(resolve, 2_000)),
  ]);
  if (child.exitCode == null) child.kill("SIGKILL");
}

export async function startCell(root, label) {
  const port = await freePort();
  const dataDir = path.join(root, label);
  const logPath = path.join(root, `${label}.log`);
  const log = openSync(logPath, "a");
  const child = spawn(BIN, ["--data-dir", dataDir, "--port", String(port)], {
    cwd: ROOT,
    // Overridable, so a failing run can be re-run with `RUST_LOG=sqlx=debug`
    // and the attached cell log answers which statement failed.
    env: { ...process.env, RUST_LOG: process.env.RUST_LOG || "info" },
    stdio: ["ignore", log, log],
  });
  const baseUrl = `http://127.0.0.1:${port}`;
  try {
    await waitForJson(`${baseUrl}/host/board/state`);
    await seedOrganSand(baseUrl);
  } catch (error) {
    await stopProcess(child);
    closeSync(log);
    throw error;
  }
  return { child, dataDir, log, logPath, baseUrl };
}


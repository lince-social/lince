import { test, expect } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { closeSync, openSync, readFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "../..");
const BIN = path.join(ROOT, "target/debug/lince");

async function freePort() {
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

async function waitForJson(url, timeout = 20_000) {
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

async function fetchJson(url, options = {}) {
  const response = await fetch(url, options);
  const text = await response.text();
  let payload = null;
  try { payload = text ? JSON.parse(text) : null; } catch { payload = text; }
  if (!response.ok) throw new Error(`${options.method || "GET"} ${url}: ${response.status} ${text}`);
  return payload;
}

async function seedOrganSand(baseUrl) {
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

async function stopProcess(child) {
  if (!child || child.exitCode != null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    new Promise((resolve) => setTimeout(resolve, 2_000)),
  ]);
  if (child.exitCode == null) child.kill("SIGKILL");
}

async function startCell(root, label) {
  const port = await freePort();
  const dataDir = path.join(root, label);
  const logPath = path.join(root, `${label}.log`);
  const log = openSync(logPath, "a");
  const child = spawn(BIN, ["--data-dir", dataDir, "--port", String(port)], {
    cwd: ROOT,
    env: { ...process.env, RUST_LOG: "info" },
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

async function startPair(browser, testInfo) {
  const root = await mkdtemp(path.join(os.tmpdir(), "lince-organ-e2e-"));
  const a = await startCell(root, "a");
  const b = await startCell(root, "b");
  const contextA = await browser.newContext();
  const contextB = await browser.newContext();
  const pageA = await contextA.newPage();
  const pageB = await contextB.newPage();
  const browserLog = [];
  for (const [name, page] of [["A", pageA], ["B", pageB]]) {
    page.on("console", (message) => browserLog.push(`${name} console ${message.type()}: ${message.text()}`));
    page.on("pageerror", (error) => browserLog.push(`${name} pageerror: ${error.stack || error}`));
    page.on("requestfailed", (request) => browserLog.push(`${name} requestfailed ${request.method()} ${request.url()}: ${request.failure()?.errorText}`));
    page.on("response", (response) => {
      if (response.status() >= 400) browserLog.push(`${name} response ${response.status()} ${response.request().method()} ${response.url()}`);
    });
  }
  await Promise.all([
    pageA.goto(a.baseUrl, { waitUntil: "domcontentloaded" }),
    pageB.goto(b.baseUrl, { waitUntil: "domcontentloaded" }),
  ]);
  await Promise.all([
    pageA.locator('iframe[title="Organ"]').waitFor(),
    pageB.locator('iframe[title="Organ"]').waitFor(),
  ]);
  return {
    root, a, b, contextA, contextB, pageA, pageB, browserLog,
    async close() {
      await Promise.allSettled([contextA.close(), contextB.close()]);
      await Promise.allSettled([stopProcess(a.child), stopProcess(b.child)]);
      closeSync(a.log);
      closeSync(b.log);
      await new Promise((resolve) => setTimeout(resolve, 50));
      for (const cell of [a, b]) {
        let body = "";
        try { body = readFileSync(cell.logPath, "utf8"); } catch {}
        await testInfo.attach(`${path.basename(cell.logPath)}`, { body, contentType: "text/plain" });
      }
      await testInfo.attach("browser.log", { body: browserLog.join("\n"), contentType: "text/plain" });
      if (testInfo.status === testInfo.expectedStatus) await rm(root, { recursive: true, force: true });
    },
  };
}

function organFrame(page) {
  return page.frameLocator('iframe[title="Organ"]');
}

function recordFrame(page) {
  return page.frameLocator('iframe[title="Record"]');
}

async function selectLocalOrganAndReadNodeId(page) {
  const frame = organFrame(page);
  await frame.locator("#organs li").first().click();
  await expect(frame.locator("#profile-panel")).toBeVisible();
  await expect(frame.locator("#pf-invite")).toHaveValue(/^lince1\|[^|]+\|/);
  const invite = await frame.locator("#pf-invite").inputValue();
  return invite.split("|")[1];
}

async function enableUnknownConversations(page, dataDir) {
  const frame = organFrame(page);
  await selectLocalOrganAndReadNodeId(page);
  await frame.locator("#dc-accept-unknown").evaluate((input) => { input.checked = true; });
  await frame.locator("#dc-save").click({ force: true });
  await expect.poll(() => sqliteQuery(dataDir,
    "SELECT json_extract(fds, '$.accept_unknown') FROM record_extension WHERE namespace = 'lince.discovery';"),
  ).toBe("1");
}

async function waitForPeer(baseUrl, nodeId) {
  let found = null;
  await expect.poll(async () => {
    const payload = await fetchJson(`${baseUrl}/organ/nearby`);
    found = payload.peers.find((peer) => peer.node_id === nodeId) || null;
    return Boolean(found);
  }, { timeout: 25_000, intervals: [250, 500, 1_000] }).toBe(true);
  return found;
}

function nearbyRow(page, fingerprint) {
  return organFrame(page).locator("#nb-list li", { hasText: fingerprint });
}

function contactState(dataDir) {
  return sqliteQuery(dataDir,
    "SELECT trust || '|' || sync_out || '|' || sync_in FROM organ_contact ORDER BY rowid;",
  ).split("\n").filter(Boolean);
}

function threadState(dataDir, conversationUid) {
  return sqliteQuery(dataDir, `
    SELECT r.head || '|' || r.quantity_mantissa || 'e-' || r.quantity_scale
      FROM record r
      JOIN record_assertion a ON a.subject_uid = r.uid AND a.retracted_at IS NULL
      JOIN concept c ON c.uid = a.predicate_uid
     WHERE r.kind = 'thread' AND r.deleted_at IS NULL
       AND c.canonical_name = 'thread-of' AND a.object_uid = '${conversationUid}'
     ORDER BY r.rowid;
  `).split("\n").filter(Boolean);
}

function sqliteQuery(dataDir, query) {
  return execFileSync("sqlite3", [
    "-cmd", ".timeout 5000", path.join(dataDir, "lince.db"), query,
  ], { encoding: "utf8" }).trim();
}

test("nearby Add known persists a known contact", async ({ browser }, testInfo) => {
  const pair = await startPair(browser, testInfo);
  try {
    const bNodeId = await selectLocalOrganAndReadNodeId(pair.pageB);
    await enableUnknownConversations(pair.pageB, pair.b.dataDir);
    await selectLocalOrganAndReadNodeId(pair.pageA);
    const peer = await waitForPeer(pair.a.baseUrl, bNodeId);
    const row = nearbyRow(pair.pageA, peer.fp);
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Add known" }).click();
    await row.getByRole("textbox", { name: "Local name for this Organ" }).fill("Known B");
    const responsePromise = pair.pageA.waitForResponse((response) =>
      response.url().endsWith("/organ/pair") && response.request().method() === "POST");
    await row.getByRole("button", { name: "Add", exact: true }).click();
    const response = await responsePromise;
    expect(response.status(), await response.text()).toBe(200);
    await expect(organFrame(pair.pageA).locator("#nb-status")).toHaveText("Added as known.");
    await expect(row).toContainText("known");
    expect(contactState(pair.a.dataDir)).toEqual(["known|0|0"]);
  } finally {
    await pair.close();
  }
});

test("an unknown nearby Cell can invite, notify, open Record chat, and exchange messages", async ({ browser }, testInfo) => {
  const pair = await startPair(browser, testInfo);
  try {
    const bNodeId = await selectLocalOrganAndReadNodeId(pair.pageB);
    await enableUnknownConversations(pair.pageB, pair.b.dataDir);
    await selectLocalOrganAndReadNodeId(pair.pageA);
    const peer = await waitForPeer(pair.a.baseUrl, bNodeId);
    const row = nearbyRow(pair.pageA, peer.fp);
    await expect(row).toBeVisible();

    await row.getByRole("button", { name: "Chat" }).click();
    await row.getByRole("textbox", { name: "Conversation title" }).fill("E2E hello");
    const offerPromise = pair.pageA.waitForResponse((response) =>
      response.url().endsWith("/organ/conversation/offer") && response.request().method() === "POST");
    await row.getByRole("button", { name: "Send request" }).click();
    const offer = await offerPromise;
    expect(offer.status(), await offer.text()).toBe(200);
    await expect(organFrame(pair.pageA).locator("#nb-status")).toContainText("Request sent");

    const toast = pair.pageB.locator(".lynx-toast");
    await expect(toast).toContainText("Conversation request", { timeout: 10_000 });
    await expect(pair.pageB.locator(".board-base-tools__row")).toHaveCSS("display", "flex");
    await toast.click();
    await expect(pair.pageB.locator("#notifications-panel")).toBeVisible();
    const accept = pair.pageB.locator('[data-thread-answer="accept"]');
    const acceptPromise = pair.pageB.waitForResponse((response) =>
      response.url().includes("/host/notifications/") && response.url().endsWith("/accept"));
    await accept.click();
    const accepted = await acceptPromise;
    expect(accepted.status()).toBe(200);
    const acceptedPayload = await accepted.json();
    expect(acceptedPayload.record_id).toBeTruthy();

    await expect.poll(
      () => threadState(pair.b.dataDir, acceptedPayload.record_id),
      { timeout: 15_000 },
    ).toEqual(["E2E hello|1e-0"]);

    await expect(recordFrame(pair.pageA).locator("#f-name")).toHaveText("E2E hello");
    await expect(recordFrame(pair.pageB).locator("#f-name")).toHaveText("E2E hello");
    await expect(recordFrame(pair.pageB).locator("#thread-tabs")).toContainText("E2E hello");
    expect(contactState(pair.a.dataDir)).toEqual(["unknown|0|0"]);
    expect(contactState(pair.b.dataDir)).toEqual(["unknown|0|0"]);

    await recordFrame(pair.pageA).locator("#cm-body").fill("hey from A");
    await recordFrame(pair.pageA).locator("#cm-post").click();
    await expect(recordFrame(pair.pageB).locator("#comments-list")).toContainText("hey from A", { timeout: 15_000 });

    await recordFrame(pair.pageB).locator("#cm-body").fill("hello from B");
    await recordFrame(pair.pageB).locator("#cm-post").click();
    await expect(recordFrame(pair.pageA).locator("#comments-list")).toContainText("hello from B", { timeout: 15_000 });
  } finally {
    await pair.close();
  }
});

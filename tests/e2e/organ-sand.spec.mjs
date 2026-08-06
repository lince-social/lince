//! The Organ Sand's own shape, in a real board: which panel belongs to which
//! Organ, and which errands live behind the corner tools.
//!
//! `HTML.contains` tests in `sand/organ/mod.rs` prove a control was authored.
//! They cannot see a panel that never renders, or a button whose click is
//! swallowed — which is exactly how "Delete does nothing" and "every organ
//! shows the same thing" got shipped. This runs the sand.

import { test, expect } from "@playwright/test";
import { mkdtemp, rm } from "node:fs/promises";
import { closeSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { startCell, stopProcess } from "./cell.mjs";

async function startSingle(browser, testInfo) {
  const root = await mkdtemp(path.join(os.tmpdir(), "lince-organ-sand-"));
  const cell = await startCell(root, "a");
  const context = await browser.newContext();
  const page = await context.newPage();
  const log = [];
  page.on("console", (m) => log.push(`console ${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => log.push(`pageerror: ${e.stack || e}`));
  await page.goto(cell.baseUrl, { waitUntil: "domcontentloaded" });
  await page.locator('iframe[title="Organ"]').waitFor();
  return {
    page,
    cell,
    frame: page.frameLocator('iframe[title="Organ"]'),
    async close() {
      await context.close().catch(() => {});
      await stopProcess(cell.child);
      closeSync(cell.log);
      let body = "";
      try { body = readFileSync(cell.logPath, "utf8"); } catch {}
      await testInfo.attach("cell.log", { body, contentType: "text/plain" });
      await testInfo.attach("browser.log", { body: log.join("\n"), contentType: "text/plain" });
      if (testInfo.status === testInfo.expectedStatus) await rm(root, { recursive: true, force: true });
    },
  };
}

test("registering and devices are errands behind the corner, not panels on an Organ", async ({ browser }, testInfo) => {
  const it = await startSingle(browser, testInfo);
  try {
    const frame = it.frame;
    await frame.locator("#organs li").first().click();

    // Reading an Organ shows the Organ, and nothing about adding one.
    await expect(frame.locator("#organ-mode")).toBeVisible();
    await expect(frame.locator("#add-panel")).toBeHidden();
    await expect(frame.locator("#devices-panel")).toBeHidden();
    await expect(frame.locator("#profile-panel")).toBeVisible();
    await expect(frame.locator("#d-head")).toContainText("this Cell");
    await testInfo.attach("organ-mode.png", {
      body: await it.page.screenshot(), contentType: "image/png",
    });

    // This Cell's own Organ cannot be deleted — `ensure_local` rebuilds it on
    // every boot, so offering the button would be offering a no-op.
    await expect(frame.locator("#o-delete")).toBeHidden();

    await frame.locator(".sand-tools").hover();
    await frame.locator("#register-open").click();
    await expect(frame.locator("#add-panel")).toBeVisible();
    await expect(frame.locator("#organ-mode")).toBeHidden();
    await testInfo.attach("register-mode.png", {
      body: await it.page.screenshot(), contentType: "image/png",
    });
    await frame.locator("#register-close").click();
    await expect(frame.locator("#add-panel")).toBeHidden();

    await frame.locator(".sand-tools").hover();
    await frame.locator("#devices-open").click();
    await expect(frame.locator("#devices-panel")).toBeVisible();
    await expect(frame.locator("#root-key-panel")).toBeVisible();
    await expect(frame.locator("#organ-mode")).toBeHidden();
    await testInfo.attach("devices-mode.png", {
      body: await it.page.screenshot(), contentType: "image/png",
    });
    await frame.locator("#devices-close").click();
    await expect(frame.locator("#devices-panel")).toBeHidden();
  } finally {
    await it.close();
  }
});

test("adding an Organ is by pairing code only", async ({ browser }, testInfo) => {
  const it = await startSingle(browser, testInfo);
  try {
    const frame = it.frame;
    await frame.locator(".sand-tools").hover();
    await frame.locator("#register-open").click();
    // Nothing here takes a hostname: under iroh the NodeId is the address, so
    // a URL would make a row that looks reachable and is not.
    await expect(frame.locator("#add-panel")).toBeVisible();
    await expect(frame.locator("#ad-invite")).toBeVisible();
    await expect(frame.locator("#organ-form")).toHaveCount(0);
    await expect(frame.locator("#organ-url")).toHaveCount(0);

    // The refusal names the field they most likely copied from instead of
    // just saying no.
    await frame.locator("#ad-invite").fill("not-a-pairing-code");
    await frame.locator("#ad-name").fill("Nobody");
    await frame.locator("#ad-save").click();
    await expect(frame.locator("#ad-status")).toContainText("lince1|");
  } finally {
    await it.close();
  }
});

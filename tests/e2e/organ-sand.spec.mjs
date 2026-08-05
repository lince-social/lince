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
    await expect(frame.locator("#organ-form")).toBeHidden();
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
    await expect(frame.locator("#organ-form")).toBeVisible();
    await expect(frame.locator("#add-panel")).toBeVisible();
    await expect(frame.locator("#organ-mode")).toBeHidden();
    await testInfo.attach("register-mode.png", {
      body: await it.page.screenshot(), contentType: "image/png",
    });
    await frame.locator("#register-close").click();
    await expect(frame.locator("#organ-form")).toBeHidden();

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

test("a registered organ can be deleted from the sand", async ({ browser }, testInfo) => {
  const it = await startSingle(browser, testInfo);
  try {
    const frame = it.frame;
    await frame.locator(".sand-tools").hover();
    await frame.locator("#register-open").click();
    await frame.locator("#organ-name").fill("Deletable Cell");
    await frame.locator("#organ-url").fill("https://deletable.example");
    await frame.locator("#organ-create").click();
    await expect(frame.locator("#organs")).toContainText("Deletable Cell");

    await frame.locator("#organs li", { hasText: "Deletable Cell" }).click();
    await expect(frame.locator("#d-head")).toContainText("Deletable Cell");

    // The confirmation is INLINE. A sandboxed frame has no allow-modals, so a
    // window.confirm() here would be dropped by the browser and the delete
    // would never run — which is what the button used to do.
    await frame.locator("#o-delete").click();
    await frame.getByRole("button", { name: "Delete", exact: true }).click();
    await expect(frame.locator("#organs")).not.toContainText("Deletable Cell");
  } finally {
    await it.close();
  }
});

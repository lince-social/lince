//! The Record Sand's shape in a real board.
//!
//! Every requirement here is geometry — a scroller that scrolls, fields that
//! share a line, sections that sort by whether the record has anything in
//! them. `HTML.contains` in `sand/record/mod.rs` proves the CSS was authored;
//! only a browser proves it applies. The scroll bug this covers was authored
//! CSS that never fired, because the flex item it sat on could not shrink.

import { test, expect } from "@playwright/test";
import { mkdtemp, rm } from "node:fs/promises";
import { closeSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { startCell, stopProcess } from "./cell.mjs";

async function openRecord(browser, testInfo, { body = "" } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "lince-record-sand-"));
  const cell = await startCell(root, "a");
  const context = await browser.newContext();
  const page = await context.newPage();
  const log = [];
  page.on("console", (m) => log.push(`console ${m.type()}: ${m.text()}`));
  page.on("pageerror", (e) => log.push(`pageerror: ${e.stack || e}`));
  await page.goto(cell.baseUrl, { waitUntil: "domcontentloaded" });

  const sand = page.frameLocator('iframe[title="Record"]');
  await sand.locator("#icon").waitFor();
  // The sand is idle until an ABI event wakes it. `frameLocator` cannot
  // evaluate, so reach the Frame through an element that lives in it.
  const frame = await (await sand.locator("body").elementHandle()).ownerFrame();
  await frame.evaluate(() => openCreate({}));
  await sand.locator("#c-head").fill("A record with a long body");
  if (body) await sand.locator("#c-body").fill(body);
  await sand.locator("#c-submit").click();
  await expect(sand.locator("#focus")).toBeVisible();

  return {
    page, frame, sand,
    async close() {
      await context.close().catch(() => {});
      await stopProcess(cell.child);
      closeSync(cell.log);
      let cellLog = "";
      try { cellLog = readFileSync(cell.logPath, "utf8"); } catch {}
      await testInfo.attach("cell.log", { body: cellLog, contentType: "text/plain" });
      await testInfo.attach("browser.log", { body: log.join("\n"), contentType: "text/plain" });
      if (testInfo.status === testInfo.expectedStatus) await rm(root, { recursive: true, force: true });
    },
  };
}

test("the panel scrolls inside the card instead of running off it", async ({ browser }, testInfo) => {
  const it = await openRecord(browser, testInfo, {
    body: Array.from({ length: 80 }, (_, at) => `line ${at}`).join("\n"),
  });
  try {
    const scroller = await it.frame.evaluate(() => {
      const element = document.getElementById("scroll");
      return {
        scrollHeight: element.scrollHeight,
        clientHeight: element.clientHeight,
        overflowY: getComputedStyle(element).overflowY,
        // The whole point: the card itself never grows a scrollbar.
        bodyOverflows: document.body.scrollHeight > document.body.clientHeight,
      };
    });
    expect(scroller.overflowY).toBe("auto");
    expect(scroller.scrollHeight).toBeGreaterThan(scroller.clientHeight);
    expect(scroller.bodyOverflows).toBe(false);

    // And it actually moves — CSS that computes right can still sit on a flex
    // item that refuses to shrink.
    const moved = await it.frame.evaluate(() => {
      const element = document.getElementById("scroll");
      element.scrollTop = 200;
      return element.scrollTop;
    });
    expect(moved).toBeGreaterThan(0);

    await testInfo.attach("record.png", { body: await it.page.screenshot(), contentType: "image/png" });
  } finally {
    await it.close();
  }
});

test("related fields sit on one line and the head reads as a title", async ({ browser }, testInfo) => {
  const it = await openRecord(browser, testInfo);
  try {
    const layout = await it.frame.evaluate(() => {
      for (const section of document.querySelectorAll("#focus details")) section.open = true;
      const top = (id) => Math.round(document.getElementById(id).getBoundingClientRect().top);
      return {
        first: document.querySelector("#focus > .t").firstElementChild.id,
        headBorder: getComputedStyle(document.getElementById("f-head")).borderTopWidth,
        slugQty: ["f-slug", "f-quantity"].map(top),
        schedule: ["w-start", "w-due", "w-estimate"].map(top),
        timer: ["w-play", "w-pause", "w-log-start", "w-log-end"].map(top),
      };
    });
    // The head is the first thing in the panel, and it is not in a box.
    expect(layout.first).toBe("f-head");
    expect(layout.headBorder).toBe("0px");
    const oneLine = (tops) => Math.max(...tops) - Math.min(...tops) <= 2;
    expect(oneLine(layout.slugQty)).toBe(true);
    expect(oneLine(layout.schedule)).toBe(true);
    // Play/Pause share their line with the manual entry they duplicate. At the
    // card's default width "Add log" wraps below rather than overflowing.
    expect(oneLine(layout.timer)).toBe(true);
  } finally {
    await it.close();
  }
});

test("sections sort by whether this record has anything in them", async ({ browser }, testInfo) => {
  const it = await openRecord(browser, testInfo);
  try {
    const read = () => it.frame.evaluate(() => {
      const out = {};
      for (const id of ["sec-worklog", "sec-estimate", "sec-people", "sec-links", "sec-comments", "sec-facts"]) {
        const element = document.getElementById(id);
        out[id] = { order: Number(getComputedStyle(element).order), open: element.open, hidden: element.hidden };
      }
      return out;
    });

    // A brand new record has none of the five, so the default accordion view
    // hides them. History remains available but starts closed.
    const empty = await read();
    for (const [at, id] of ["sec-worklog", "sec-estimate", "sec-people", "sec-links", "sec-comments"].entries()) {
      expect(empty[id]).toEqual({ order: 10 + at, open: false, hidden: true });
    }
    expect(empty["sec-facts"]).toEqual({ order: 70, open: false, hidden: false });

    // The left half reveals every property; the right half hides every one.
    await it.sand.locator("#properties-more").click();
    expect((await read())["sec-worklog"].hidden).toBe(false);
    await it.sand.locator("#properties-hide").click();
    expect((await read())["sec-facts"].hidden).toBe(true);
    await it.sand.locator("#properties-hide").click();

    // Start a thread and Threads rises into the filled band and opens itself.
    await it.sand.locator("#properties-more").click();
    await it.frame.evaluate(() => { document.getElementById("sec-comments").open = true; });
    await it.sand.locator("#cm-body").fill("first message");
    await it.sand.locator("#cm-post").click();
    await expect(it.sand.locator("#comments-list")).toContainText("first message");
    await it.sand.locator("#properties-more").click();

    const filled = await read();
    expect(filled["sec-comments"]).toEqual({ order: 14, open: true, hidden: false });
    expect(filled["sec-worklog"].hidden).toBe(true);
    expect(filled["sec-estimate"].hidden).toBe(true);
    expect(filled["sec-people"].hidden).toBe(true);
    // The thread is a real link off this record, so Links fills with it — and
    // it still sorts above Threads, which is the order that was asked for.
    expect(filled["sec-links"].order).toBeLessThan(filled["sec-comments"].order);
  } finally {
    await it.close();
  }
});

test("body modes replace the split textarea and preview", async ({ browser }, testInfo) => {
  const source = "## Plan\n- [ ] one\nplain\n```mermaid\ngraph LR\nA --> B\n```";
  const it = await openRecord(browser, testInfo, { body: source });
  try {
    await expect(it.sand.locator("#f-body")).toBeHidden();
    await expect(it.sand.locator("#f-preview .md-h2")).toBeVisible();

    await it.sand.locator('#f-preview [data-md-line="2"]').click();
    const active = it.sand.locator(".pragmatic-source");
    await expect(active).toHaveValue("plain");
    await active.fill("changed");
    await expect(it.sand.locator("#f-save")).toBeEnabled();

    await it.sand.locator("#body-pretty").click();
    await expect(it.sand.locator(".pragmatic-source")).toHaveCount(0);
    await expect(it.sand.locator('#f-preview input[data-md-line="1"]')).toBeDisabled();

    // Any line in a fenced block reveals that complete block as source, then
    // returns to pretty after five seconds without caret activity.
    await it.sand.locator("#body-pragmatic").click();
    await it.sand.locator("#f-preview .md-mermaid").click();
    await expect(active).toHaveValue("```mermaid\ngraph LR\nA --> B\n```");
    await expect(active).toHaveCount(0, { timeout: 7000 });

    await it.sand.locator("#body-raw").click();
    await expect(it.sand.locator("#f-body")).toBeVisible();
    await expect(it.sand.locator("#f-body")).toHaveValue(source.replace("plain", "changed"));
  } finally {
    await it.close();
  }
});

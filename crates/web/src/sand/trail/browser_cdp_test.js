const port = Number(process.argv[1]);
const boardUrl = String(process.argv[2] || "");
const watchMode = String(process.argv[3] || "") === "watch";

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await predicate();
    if (value) {
      return value;
    }
    await sleep(50);
  }
  throw new Error("Timed out waiting for " + label);
}

async function main() {
  const target = await waitFor(async () => {
    const response = await fetch("http://127.0.0.1:" + port + "/json/list");
    if (!response.ok) {
      return null;
    }
    const list = await response.json();
    return list.find((item) => item.type === "page" && item.webSocketDebuggerUrl) || null;
  }, 15000, "devtools target");

  const websocket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    websocket.onopen = () => resolve();
    websocket.onerror = () => reject(new Error("Failed to connect to Chromium DevTools"));
  });

  let nextId = 0;
  const pending = new Map();
  let loadResolve = null;

  function waitForLoadEvent() {
    return new Promise((resolve) => {
      loadResolve = resolve;
    });
  }

  websocket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id) {
      const pendingRequest = pending.get(message.id);
      if (!pendingRequest) {
        return;
      }
      pending.delete(message.id);
      if (message.error) {
        pendingRequest.reject(new Error(message.error.message || JSON.stringify(message.error)));
        return;
      }
      pendingRequest.resolve(message);
      return;
    }
    if (message.method === "Runtime.consoleAPICalled") {
      const args = (message.params?.args || []).map((arg) => arg.value ?? arg.description ?? "").join(" ");
      console.error("console[" + (message.params?.type || "log") + "]: " + args);
      return;
    }
    if (message.method === "Runtime.exceptionThrown") {
      console.error("exception: " + JSON.stringify(message.params?.exceptionDetails || message.params || {}));
      return;
    }
    if (message.method === "Log.entryAdded") {
      const entry = message.params?.entry || {};
      console.error("log[" + (entry.level || "info") + "]: " + (entry.text || ""));
      return;
    }
    if (message.method === "Network.responseReceived") {
      const response = message.params?.response || {};
      if ((response.status || 0) >= 400) {
        console.error("http[" + response.status + "]: " + (response.url || ""));
      }
      return;
    }
    if (message.method === "Network.loadingFailed") {
      console.error("network failed: " + JSON.stringify(message.params || {}));
      return;
    }
    if (message.method === "Page.loadEventFired" && loadResolve) {
      loadResolve();
      loadResolve = null;
    }
  };

  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++nextId;
      pending.set(id, { resolve, reject });
      websocket.send(JSON.stringify({ id, method, params }));
    });
  }

  await send("Page.enable");
  await send("Runtime.enable");
  await send("Log.enable");
  await send("Network.enable");
  const loadPromise = waitForLoadEvent();
  await send("Page.navigate", { url: boardUrl });
  await Promise.race([
    loadPromise,
    sleep(30000).then(() => {
      throw new Error("Timed out waiting for page load");
    }),
  ]);

  function buildPageScript(phase, expectedTrailRootId = null) {
    return String.raw`(
      async () => {
        const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
        const watchMode = ${watchMode ? "true" : "false"};
        const phase = ${JSON.stringify(phase)};
        const expectedTrailRootId = ${expectedTrailRootId === null ? "null" : Number(expectedTrailRootId)};

        async function waitFor(predicate, timeoutMs, label) {
          const deadline = Date.now() + timeoutMs;
          while (Date.now() < deadline) {
            const value = await predicate();
            if (value) {
              return value;
            }
            await sleep(50);
          }
          throw new Error("Timed out waiting for " + label);
        }

        async function expectStable(predicate, timeoutMs, label) {
          await waitFor(predicate, timeoutMs, label);
          const deadline = Date.now() + timeoutMs;
          while (Date.now() < deadline) {
            if (!await predicate()) {
              throw new Error("State regressed while waiting for " + label);
            }
            await sleep(50);
          }
        }

        let startupStateLogged = false;
        function findTrailFrame() {
          const frame = document.querySelector('iframe.package-widget__frame[data-package-instance-id="trail-browser-widget"]');
          if (!frame) {
            return null;
          }
          frame.scrollIntoView({ block: "center", inline: "center" });
          return frame;
        }

        function trailDoc() {
          const frame = findTrailFrame();
          return frame && frame.contentDocument ? frame.contentDocument : null;
        }

        function byId(doc, id) {
          return doc.getElementById(id);
        }

        function nodeGroupById(doc, id) {
          return doc.querySelector('g[data-node-id="' + Number(id) + '"]');
        }

        function selectionTitle() {
          return String(byId(doc, "trail-selection-title")?.textContent || "");
        }

        function rowCountText() {
          return String(byId(doc, "trail-row-pill")?.textContent || "").trim();
        }

        function nodeFill(nodeId) {
          return doc.querySelector('g[data-node-id="' + Number(nodeId) + '"] circle')?.getAttribute("fill") || "";
        }

        async function watchPause(ms) {
          if (watchMode) {
            await sleep(ms);
          }
        }

        async function clickNodeAndWait(nodeId, label) {
          const node = await waitFor(
            () => nodeGroupById(doc, nodeId),
            30000,
            label + " node",
          );
          node.dispatchEvent(new MouseEvent("click", {
            bubbles: true,
            cancelable: true,
            view: doc.defaultView,
          }));
          await watchPause(350);
        }

        async function expectNodeFill(nodeId, expectedFill, label) {
          await waitFor(
            () => {
              const fill = nodeFill(nodeId);
              return fill === expectedFill ? fill : null;
            },
            30000,
            label + " to render with fill " + expectedFill,
          );
        }

        async function clickSelectedNodeDone(label) {
          byId(doc, "trail-set-done").click();
          await watchPause(600);
          await waitFor(
            () => {
              const status = String(byId(doc, "trail-status-pill")?.textContent || "").trim();
              return status.includes("Node updated") ? status : null;
            },
            30000,
            label + " update to persist",
          );
          await expectStable(
            () => selectionTitle().includes("Done"),
            5000,
            label + " selection to stay Done",
          );
          const title = selectionTitle();
          if (!title.includes("Done")) {
            throw new Error(label + " should show Done, got: " + title);
          }
        }

        async function openTrail(doc, rootId) {
          const card = await waitFor(
            () => doc.querySelector('article[data-testid="trail-discover-card"][data-original-id="' + rootId + '"]'),
            30000,
            "discover result card " + rootId,
          );
          const button = card.querySelector('[data-testid="trail-open-root"]');
          if (!button) {
            throw new Error("Missing Open trail button for record " + rootId);
          }
          button.click();
          await watchPause(600);
          await waitFor(
            () => byId(doc, "trail-bound-pill")?.textContent?.includes("#" + rootId),
            30000,
            "trail binding for " + rootId,
          );
          await waitFor(
            () => {
              const status = String(byId(doc, "trail-status-pill")?.textContent || "").trim();
              return status === "Live" ? status : null;
            },
            30000,
            "trail stream live for " + rootId,
          );
        }

        const doc = await waitFor(
          () => {
            if (!startupStateLogged) {
              console.error(
                "app class=" + document.body.className +
                " startupHidden=" + String(document.getElementById("startup-screen")?.hidden)
              );
              startupStateLogged = true;
            }
            return document.body.classList.contains("startup-active") ? null : trailDoc();
          },
          30000,
          "trail widget iframe",
        );
        console.error("frame readyState=" + doc.readyState + " body=" + String(doc.body?.innerHTML || "").slice(0, 300));

        async function waitForReady() {
          await waitFor(
            () => {
              const status = String(byId(doc, "trail-status-pill")?.textContent || "").trim();
              return status && status !== "Booting" && status !== "Loading" ? status : null;
            },
            30000,
            "trail widget ready",
          );
          await waitFor(
            () => {
              const results = byId(doc, "trail-discover-results");
              return results && results.children.length > 0 ? results : null;
            },
            30000,
            "trail discover panel",
          );
        }

        async function runInitialPhase() {
          await waitForReady();

          await openTrail(doc, 1);
          await waitFor(
            () => nodeGroupById(doc, 1),
            30000,
            "alpha root node",
          );
          await expectNodeFill(1, "#7ef0c6", "alpha root");
          await clickNodeAndWait(1, "alpha root");
          await clickSelectedNodeDone("alpha root");
          await waitFor(
            () => nodeGroupById(doc, 2),
            30000,
            "alpha child node to become visible",
          );
          await expectNodeFill(2, "#f2bb78", "alpha child");
          const alphaRows = rowCountText();
          if (!alphaRows.includes("2 nodes")) {
            throw new Error("Alpha trail should show 2 nodes, got: " + alphaRows);
          }

          await openTrail(doc, 30);
          await waitFor(
            () => nodeGroupById(doc, 30),
            30000,
            "chain root node",
          );
          await expectNodeFill(30, "#7ef0c6", "chain root");
          await clickNodeAndWait(30, "chain root");
          await clickSelectedNodeDone("chain root");
          await waitFor(
            () => nodeGroupById(doc, 31),
            30000,
            "chain child node to become visible",
          );
          await expectNodeFill(31, "#f2bb78", "chain child");
          const chainRowsAfterRoot = rowCountText();
          if (!chainRowsAfterRoot.includes("2 nodes")) {
            throw new Error("Chain trail should show 2 nodes after root completion, got: " + chainRowsAfterRoot);
          }
          await clickNodeAndWait(31, "chain child");
          await clickSelectedNodeDone("chain child");

          await waitFor(
            () => nodeGroupById(doc, 32),
            30000,
            "chain grandchild node to become visible",
          );
          await expectNodeFill(32, "#f2bb78", "chain grandchild");
          const chainRowsAfterChild = rowCountText();
          if (!chainRowsAfterChild.includes("3 nodes")) {
            throw new Error("Chain trail should show 3 nodes after child completion, got: " + chainRowsAfterChild);
          }
          await clickNodeAndWait(32, "chain grandchild");
          await clickSelectedNodeDone("chain grandchild");

          await waitFor(
            () => nodeGroupById(doc, 33),
            30000,
            "chain great-grandchild node to become visible",
          );
          await expectNodeFill(33, "#f2bb78", "chain great-grandchild");
          const chainRows = rowCountText();
          if (!chainRows.includes("4 nodes")) {
            throw new Error("Chain trail should show 4 nodes, got: " + chainRows);
          }

          return { lastTrailRootId: 30 };
        }

        async function runRefreshPhase() {
          await waitForReady();
          await waitFor(
            () => {
              const status = String(byId(doc, "trail-status-pill")?.textContent || "").trim();
              return status === "Live" ? status : null;
            },
            30000,
            "restored trail stream live",
          );
          await waitFor(
            () => {
              const bound = String(byId(doc, "trail-bound-pill")?.textContent || "");
              return bound.includes("#" + expectedTrailRootId) ? bound : null;
            },
            30000,
            "restored trail binding",
          );
          return "pass";
        }

        if (phase === "initial") {
          return await runInitialPhase();
        }
        if (phase === "refresh") {
          return await runRefreshPhase();
        }
        throw new Error("Unknown browser test phase: " + phase);
      }
    )()`;
  }

  async function evaluatePage(phase, expectedTrailRootId = null) {
    const evaluation = await send("Runtime.evaluate", {
      expression: buildPageScript(phase, expectedTrailRootId),
      awaitPromise: true,
      returnByValue: true,
    });

    if (evaluation.result?.exceptionDetails) {
      const description =
        evaluation.result.exceptionDetails.exception?.description ||
        evaluation.result.exceptionDetails.text ||
        "Chromium evaluation failed";
      throw new Error(description);
    }

    return evaluation.result?.result?.value;
  }

  async function reloadPage() {
    const reloadPromise = waitForLoadEvent();
    await send("Page.reload", { ignoreCache: true });
    await Promise.race([
      reloadPromise,
      sleep(30000).then(() => {
        throw new Error("Timed out waiting for page reload");
      }),
    ]);
  }

  const initialValue = await evaluatePage("initial");
  if (!initialValue || initialValue.lastTrailRootId !== 30) {
    throw new Error("Unexpected initial browser result: " + JSON.stringify(initialValue));
  }

  await reloadPage();

  const refreshValue = await evaluatePage("refresh", 30);
  if (refreshValue !== "pass") {
    throw new Error("Unexpected refresh browser result: " + JSON.stringify(refreshValue));
  }

  console.log("pass");
  await sleep(100);
  websocket.close();
  await sleep(100);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});

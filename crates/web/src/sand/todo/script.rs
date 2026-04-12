pub(super) fn script() -> String {
    r##"
      (() => {
        const frame = window.frameElement;
        const appRoot = document.getElementById("app");
        const statusPill = document.getElementById("table-status");
        const tableDetails = document.getElementById("table-details");
        const bootstrap = document.getElementById("table-stream-bootstrap");
        const tablePanel = document.getElementById("table-body");
        const blobLayer = document.getElementById("todo-blob-layer");
        const blobEnabledInput = document.getElementById("blob-enabled");
        const blobViscosityInput = document.getElementById("blob-viscosity");
        const blobEnergyInput = document.getElementById("blob-energy");
        const blobColorInput = document.getElementById("blob-color-input");
        const blobAddColorButton = document.getElementById("blob-add-color");
        const blobPalette = document.getElementById("blob-palette");
        const datastarReady = import("/static/vendored/datastar.js").catch(() => null);
        const serverId = String(frame?.dataset?.linceServerId || "").trim();
        const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
        const settingsKey = "todo-mode/" + instanceId;
        const blobSettingsKey = "todo-blob/" + instanceId;
        const defaultBlobColors = ["#51f3d2", "#7cc7ff", "#f5d36a"];

        const state = {
          controller: null,
          reconnectTimer: null,
          reconnectAttempt: 0,
          scrollTimer: null,
          streamGeneration: 0,
          streamUrl: "",
          focusedRowIndex: 0,
          nerdMode: false,
          quantityHistory: [],
          quantityHistoryCursor: -1,
          blobSettings: {
            enabled: true,
            viscosity: 0.62,
            energy: 0.56,
            colors: defaultBlobColors.slice(),
          },
          blob: {
            adapter: null,
            device: null,
            context: null,
            canvas: null,
            pipeline: null,
            bindGroup: null,
            uniformBuffer: null,
            uniformData: new Float32Array(24),
            ready: false,
            setupPending: false,
            frameId: 0,
            visible: 0,
            current: { x: 0, y: 0 },
            target: { x: 0, y: 0 },
            phase: "idle",
            phaseStarted: 0,
            origin: null,
            width: 0,
            height: 0,
            dpr: 1,
          },
          chromeTimer: null,
        };

        const MAX_HISTORY = 100;

        function clamp(value, min, max) {
          return Math.min(max, Math.max(min, value));
        }

        function lerp(from, to, amount) {
          return from + (to - from) * amount;
        }

        function easeOutCubic(value) {
          const next = clamp(value, 0, 1) - 1;
          return next * next * next + 1;
        }

        function setStatus(text, tone = "idle") {
          if (!statusPill) {
            return;
          }

          statusPill.dataset.tone = tone;
          statusPill.setAttribute("aria-label", text);
          statusPill.title = text;
          statusPill.textContent = "";
        }

        function setChromeVisible(visible) {
          if (!appRoot) {
            return;
          }

          if (visible) {
            appRoot.dataset.pointerActive = "true";
            if (state.chromeTimer) {
              window.clearTimeout(state.chromeTimer);
            }
            state.chromeTimer = window.setTimeout(() => {
              delete appRoot.dataset.pointerActive;
              state.chromeTimer = null;
            }, 900);
            return;
          }

          delete appRoot.dataset.pointerActive;
          if (state.chromeTimer) {
            window.clearTimeout(state.chromeTimer);
            state.chromeTimer = null;
          }
        }

        function readSettings() {
          try {
            const raw = window.localStorage?.getItem?.(settingsKey);
            if (!raw) {
              return null;
            }

            const parsed = JSON.parse(raw);
            if (!parsed || typeof parsed !== "object") {
              return null;
            }

            return {
              nerdMode: String(parsed.mode || "common") === "helix",
            };
          } catch {
            return null;
          }
        }

        function writeSettings() {
          try {
            window.localStorage?.setItem?.(
              settingsKey,
              JSON.stringify({
                mode: state.nerdMode ? "helix" : "common",
              }),
            );
          } catch {
            // ignore storage failures
          }
        }

        function normalizeHexColor(value) {
          const match = String(value || "").trim().match(/^#?([0-9a-f]{6})$/i);
          return match ? "#" + match[1].toLowerCase() : null;
        }

        function hexToRgb(color) {
          const normalized = normalizeHexColor(color) || defaultBlobColors[0];
          const value = Number.parseInt(normalized.slice(1), 16);
          return [
            ((value >> 16) & 255) / 255,
            ((value >> 8) & 255) / 255,
            (value & 255) / 255,
          ];
        }

        function readBlobSettings() {
          try {
            const raw = window.localStorage?.getItem?.(blobSettingsKey);
            if (!raw) {
              return null;
            }

            const parsed = JSON.parse(raw);
            if (!parsed || typeof parsed !== "object") {
              return null;
            }

            const colors = Array.isArray(parsed.colors)
              ? parsed.colors.map(normalizeHexColor).filter(Boolean).slice(0, 8)
              : [];

            return {
              enabled: parsed.enabled !== false,
              viscosity: clamp(Number(parsed.viscosity ?? 0.62), 0, 1),
              energy: clamp(Number(parsed.energy ?? 0.56), 0, 1),
              colors: colors.length ? colors : defaultBlobColors.slice(),
            };
          } catch {
            return null;
          }
        }

        function writeBlobSettings() {
          try {
            window.localStorage?.setItem?.(
              blobSettingsKey,
              JSON.stringify({
                enabled: state.blobSettings.enabled,
                viscosity: state.blobSettings.viscosity,
                energy: state.blobSettings.energy,
                colors: state.blobSettings.colors,
              }),
            );
          } catch {
            // ignore storage failures
          }
        }

        function syncBlobControls() {
          if (blobEnabledInput instanceof HTMLInputElement) {
            blobEnabledInput.checked = state.blobSettings.enabled;
          }

          if (blobViscosityInput instanceof HTMLInputElement) {
            blobViscosityInput.value = String(Math.round(state.blobSettings.viscosity * 100));
          }

          if (blobEnergyInput instanceof HTMLInputElement) {
            blobEnergyInput.value = String(Math.round(state.blobSettings.energy * 100));
          }

          if (blobColorInput instanceof HTMLInputElement) {
            blobColorInput.value = state.blobSettings.colors[0] || defaultBlobColors[0];
          }
        }

        function renderBlobPalette() {
          if (!blobPalette) {
            return;
          }

          blobPalette.replaceChildren();
          for (const color of state.blobSettings.colors) {
            const swatch = document.createElement("button");
            swatch.type = "button";
            swatch.className = "paletteSwatch";
            swatch.dataset.color = color;
            swatch.style.background = color;
            swatch.title = "Remove " + color;
            swatch.setAttribute("aria-label", "Remove blob color " + color);
            blobPalette.appendChild(swatch);
          }
        }

        function currentModeSelect() {
          return document.getElementById("table-mode");
        }

        function syncModeSelect() {
          const select = currentModeSelect();
          if (!select) {
            return;
          }

          select.value = state.nerdMode ? "helix" : "common";
        }

        function syncBlobMode() {
          if (!tablePanel) {
            return;
          }

          if (state.nerdMode && state.blobSettings.enabled && state.blob.ready) {
            tablePanel.dataset.blob = "true";
          } else {
            delete tablePanel.dataset.blob;
          }

          if (state.blobSettings.enabled) {
            void setupBlobLayer();
          }
        }

        function buildStreamUrl() {
          if (!serverId) {
            return "";
          }

          if (!Number.isInteger(viewId) || viewId <= 0) {
            return "";
          }

          return (
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/views/" +
            encodeURIComponent(viewId) +
            "/table/stream"
          );
        }

        function parseEventBlock(block) {
          const lines = block.split("\n");
          let eventName = "message";
          const dataLines = [];

          for (const line of lines) {
            if (line.startsWith("event:")) {
              eventName = line.slice(6).trim();
            } else if (line.startsWith("data:")) {
              dataLines.push(line.slice(5).trimStart());
            }
          }

          return { event: eventName, data: dataLines.join("\n") };
        }

        function parseSsePayload(payload) {
          if (typeof payload !== "string") {
            return payload;
          }

          try {
            return JSON.parse(payload);
          } catch {
            return payload;
          }
        }

        function dispatchDatastarEvent(eventName, detail) {
          document.dispatchEvent(
            new CustomEvent("datastar-fetch", {
              detail: {
                type: eventName,
                argsRaw: detail,
              },
            }),
          );
        }

        function stopReconnectTimer() {
          if (state.reconnectTimer) {
            window.clearTimeout(state.reconnectTimer);
            state.reconnectTimer = null;
          }
        }

        function stopScrollTimer() {
          if (state.scrollTimer) {
            window.clearTimeout(state.scrollTimer);
            state.scrollTimer = null;
          }
        }

        function setScrolling(active) {
          if (!tablePanel) {
            return;
          }

          if (active) {
            tablePanel.dataset.scrolling = "true";
            stopScrollTimer();
            state.scrollTimer = window.setTimeout(() => {
              delete tablePanel.dataset.scrolling;
              state.scrollTimer = null;
            }, 160);
            return;
          }

          delete tablePanel.dataset.scrolling;
          stopScrollTimer();
        }

        function clearSelectionAttributes() {
          if (!tablePanel) {
            return;
          }

          tablePanel.querySelectorAll("[data-row-focused]").forEach((node) => {
            delete node.dataset.rowFocused;
          });
        }

        function rows() {
          return tablePanel
            ? Array.from(tablePanel.querySelectorAll("tbody tr"))
            : [];
        }

        function focusedRow() {
          return rows()[state.focusedRowIndex] || null;
        }

        function focusedRecordId() {
          const row = focusedRow();
          if (!row) {
            return null;
          }

          const directId = Number(String(row.dataset.recordId || "").trim());
          if (Number.isInteger(directId) && directId > 0) {
            return directId;
          }

          const key = String(row.dataset.rowKey || "").trim();
          const match = key.match(/^id:(-?\d+)$/);
          if (!match) {
            return null;
          }

          const recordId = Number(match[1]);
          return Number.isFinite(recordId) ? recordId : null;
        }

        function findRowByRecordId(recordId) {
          if (!tablePanel || !Number.isFinite(recordId)) {
            return null;
          }

          return tablePanel.querySelector(
            'tbody tr[data-record-id="' + String(recordId) + '"]',
          );
        }

        function readRowQuantity(row) {
          if (!row) {
            return null;
          }

          const raw = String(row.dataset.rowQuantity || "").trim();
          if (raw) {
            const parsed = Number(raw);
            if (Number.isFinite(parsed)) {
              return parsed;
            }
          }

          const cell = row.querySelector('td[data-column-key="quantity"] .cellValue');
          const fallback = Number(String(cell?.textContent || "").trim());
          return Number.isFinite(fallback) ? fallback : null;
        }

        function applyRowQuantity(row, quantity) {
          if (!row || !Number.isFinite(quantity)) {
            return;
          }

          row.dataset.rowQuantity = String(quantity);
          const quantityCell = row.querySelector('td[data-column-key="quantity"] .cellValue');
          if (quantityCell) {
            quantityCell.textContent = String(quantity);
          }
        }

        function pruneQuantityHistoryTail() {
          if (state.quantityHistoryCursor >= state.quantityHistory.length - 1) {
            return;
          }

          state.quantityHistory.splice(state.quantityHistoryCursor + 1);
        }

        function pushQuantityHistory(entry) {
          pruneQuantityHistoryTail();
          state.quantityHistory.push(entry);

          if (state.quantityHistory.length > MAX_HISTORY) {
            const overflow = state.quantityHistory.length - MAX_HISTORY;
            state.quantityHistory.splice(0, overflow);
          }

          state.quantityHistoryCursor = state.quantityHistory.length - 1;
        }

        function resizeBlobCanvas() {
          if (!blobLayer || !state.blob.canvas) {
            return { width: 0, height: 0 };
          }

          const rect = blobLayer.getBoundingClientRect();
          const cssWidth = Math.max(1, rect.width);
          const cssHeight = Math.max(1, rect.height);
          const dpr = Math.min(window.devicePixelRatio || 1, 2);
          const pixelWidth = Math.max(1, Math.round(cssWidth * dpr));
          const pixelHeight = Math.max(1, Math.round(cssHeight * dpr));

          state.blob.canvas.style.width = cssWidth + "px";
          state.blob.canvas.style.height = cssHeight + "px";

          if (
            state.blob.canvas.width !== pixelWidth ||
            state.blob.canvas.height !== pixelHeight
          ) {
            state.blob.canvas.width = pixelWidth;
            state.blob.canvas.height = pixelHeight;
          }

          state.blob.width = cssWidth;
          state.blob.height = cssHeight;
          state.blob.dpr = dpr;
          return { width: cssWidth, height: cssHeight };
        }

        function blobPointForRow(row) {
          if (!blobLayer || !row) {
            return null;
          }

          const cell =
            row.querySelector('td[data-column-key="head"]') ||
            row.querySelector("td") ||
            row;
          const cellRect = cell.getBoundingClientRect();
          const layerRect = blobLayer.getBoundingClientRect();

          if (cellRect.height <= 0 || cellRect.width <= 0) {
            return null;
          }

          return {
            x: cellRect.left - layerRect.left + 2,
            y: cellRect.top - layerRect.top + cellRect.height / 2,
          };
        }

        function startCompletionBlob(origin) {
          if (!blobLayer || !state.blobSettings.enabled || !origin) {
            return;
          }

          state.blob.phase = "completion";
          state.blob.phaseStarted = performance.now();
          state.blob.origin = { x: origin.x, y: origin.y };
          state.blob.current = { x: origin.x, y: origin.y };
          state.blob.target = { x: origin.x, y: origin.y };
          state.blob.visible = 1;
          void setupBlobLayer();
        }

        function updateCursorBlobTarget() {
          if (!state.nerdMode || !state.blobSettings.enabled) {
            if (state.blob.phase === "cursor") {
              state.blob.phase = "idle";
            }
            return false;
          }

          const target = blobPointForRow(focusedRow());
          if (!target) {
            if (state.blob.phase === "cursor") {
              state.blob.phase = "idle";
            }
            return false;
          }

          if (state.blob.phase !== "cursor" || state.blob.visible < 0.05) {
            state.blob.current = { x: target.x, y: target.y };
          }

          state.blob.phase = "cursor";
          state.blob.target = target;
          return true;
        }

        function colorsForUniform() {
          const colors = state.blobSettings.colors.length
            ? state.blobSettings.colors
            : defaultBlobColors;

          return [
            hexToRgb(colors[0] || defaultBlobColors[0]),
            hexToRgb(colors[1] || colors[0] || defaultBlobColors[1]),
            hexToRgb(colors[2] || colors[1] || colors[0] || defaultBlobColors[2]),
          ];
        }

        function writeBlobUniforms(now, width, height, completion) {
          const data = state.blob.uniformData;
          const colors = colorsForUniform();

          data[0] = width;
          data[1] = height;
          data[2] = now / 1000;
          data[3] = state.blob.visible;
          data[4] = state.blob.current.x;
          data[5] = state.blob.current.y;
          data[6] = state.blob.target.x;
          data[7] = state.blob.target.y;
          data[8] = state.blobSettings.viscosity;
          data[9] = state.blobSettings.energy;
          data[10] = state.blob.phase === "completion" ? 1 : 0;
          data[11] = completion;

          for (let index = 0; index < 3; index += 1) {
            const base = 12 + index * 4;
            data[base] = colors[index][0];
            data[base + 1] = colors[index][1];
            data[base + 2] = colors[index][2];
            data[base + 3] = 1;
          }

          state.blob.device.queue.writeBuffer(state.blob.uniformBuffer, 0, data);
        }

        function renderBlobFrame(now) {
          if (!state.blob.ready) {
            return;
          }

          const { width, height } = resizeBlobCanvas();
          let active = false;
          let completion = 0;
          let targetVisible = 0;

          if (state.blob.phase === "completion") {
            const age = now - state.blob.phaseStarted;
            const origin = state.blob.origin || state.blob.current;
            const center = { x: width / 2, y: height / 2 };
            active = true;

            if (age <= 2600) {
              completion = easeOutCubic(age / 2600);
              state.blob.target = {
                x: lerp(origin.x, center.x, completion),
                y: lerp(origin.y, center.y, completion),
              };
              targetVisible = 1;
            } else if (age <= 4300) {
              completion = 1;
              state.blob.target = center;
              targetVisible = 1 - clamp((age - 2600) / 1700, 0, 1);
            } else {
              state.blob.phase = "idle";
              state.blob.origin = null;
              active = false;
            }
          } else {
            active = updateCursorBlobTarget();
            targetVisible = active && state.blobSettings.enabled ? 1 : 0;
          }

          state.blob.visible += (targetVisible - state.blob.visible) * (targetVisible > state.blob.visible ? 0.18 : 0.1);

          const follow = state.blob.phase === "completion"
            ? 0.12 + state.blobSettings.energy * 0.04
            : 0.06 + (1 - state.blobSettings.viscosity) * 0.22;
          state.blob.current.x += (state.blob.target.x - state.blob.current.x) * follow;
          state.blob.current.y += (state.blob.target.y - state.blob.current.y) * follow;

          writeBlobUniforms(now, width, height, completion);

          const encoder = state.blob.device.createCommandEncoder();
          const textureView = state.blob.context.getCurrentTexture().createView();
          const pass = encoder.beginRenderPass({
            colorAttachments: [
              {
                view: textureView,
                clearValue: { r: 0, g: 0, b: 0, a: 0 },
                loadOp: "clear",
                storeOp: "store",
              },
            ],
          });

          pass.setPipeline(state.blob.pipeline);
          pass.setBindGroup(0, state.blob.bindGroup);
          pass.draw(3);
          pass.end();
          state.blob.device.queue.submit([encoder.finish()]);

          state.blob.frameId = window.requestAnimationFrame(renderBlobFrame);
        }

        async function setupBlobLayer() {
          if (!blobLayer || state.blob.ready || state.blob.setupPending) {
            return;
          }

          if (!navigator.gpu) {
            return;
          }

          state.blob.setupPending = true;

          try {
            const adapter = await navigator.gpu.requestAdapter();
            if (!adapter) {
              return;
            }

            const device = await adapter.requestDevice();
            device.addEventListener?.("uncapturederror", (event) => {
              console.error("Todo WebGPU error", event.error);
            });
            device.lost.then((info) => {
              console.error("Todo WebGPU device lost", info);
            });

            const canvas = document.createElement("canvas");
            const context = canvas.getContext("webgpu");
            if (!context) {
              return;
            }

            canvas.setAttribute("aria-hidden", "true");
            blobLayer.appendChild(canvas);
            state.blob.canvas = canvas;
            resizeBlobCanvas();

            const format = navigator.gpu.getPreferredCanvasFormat();
            context.configure({
              device,
              format,
              alphaMode: "premultiplied",
              usage: GPUTextureUsage.RENDER_ATTACHMENT,
            });

            const shaderResponse = await fetch("blob.wgsl", { cache: "no-store" });
            if (!shaderResponse.ok) {
              throw new Error("Unable to load blob.wgsl.");
            }

            const shaderSource = await shaderResponse.text();
            const shaderModule = device.createShaderModule({ code: shaderSource });
            if (typeof shaderModule.getCompilationInfo === "function") {
              const compilationInfo = await shaderModule.getCompilationInfo();
              const errors = compilationInfo.messages.filter((message) => message.type === "error");
              if (errors.length) {
                throw new Error(
                  errors
                    .map((message) => {
                      return (
                        "WGSL " +
                        message.lineNum +
                        ":" +
                        message.linePos +
                        " " +
                        message.message
                      );
                    })
                    .join("\n"),
                );
              }
            }

            const uniformBuffer = device.createBuffer({
              size: state.blob.uniformData.byteLength,
              usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
            });

            const bindGroupLayout = device.createBindGroupLayout({
              entries: [
                {
                  binding: 0,
                  visibility: GPUShaderStage.FRAGMENT,
                  buffer: { type: "uniform" },
                },
              ],
            });

            const pipelineDescriptor = {
              layout: device.createPipelineLayout({ bindGroupLayouts: [bindGroupLayout] }),
              vertex: {
                module: shaderModule,
                entryPoint: "vs_main",
              },
              fragment: {
                module: shaderModule,
                entryPoint: "fs_main",
                targets: [
                  {
                    format,
                    blend: {
                      color: {
                        srcFactor: "one",
                        dstFactor: "one-minus-src-alpha",
                        operation: "add",
                      },
                      alpha: {
                        srcFactor: "one",
                        dstFactor: "one-minus-src-alpha",
                        operation: "add",
                      },
                    },
                  },
                ],
              },
              primitive: {
                topology: "triangle-list",
              },
            };

            let pipeline;
            if (typeof device.createRenderPipelineAsync === "function") {
              pipeline = await device.createRenderPipelineAsync(pipelineDescriptor);
            } else {
              device.pushErrorScope("validation");
              pipeline = device.createRenderPipeline(pipelineDescriptor);
              const validationError = await device.popErrorScope();
              if (validationError) {
                throw validationError;
              }
            }

            const bindGroup = device.createBindGroup({
              layout: bindGroupLayout,
              entries: [
                {
                  binding: 0,
                  resource: { buffer: uniformBuffer },
                },
              ],
            });

            state.blob.adapter = adapter;
            state.blob.device = device;
            state.blob.context = context;
            state.blob.pipeline = pipeline;
            state.blob.bindGroup = bindGroup;
            state.blob.uniformBuffer = uniformBuffer;
            state.blob.ready = true;

            syncBlobMode();
            if (!state.blob.frameId) {
              state.blob.frameId = window.requestAnimationFrame(renderBlobFrame);
            }
          } catch (error) {
            if (error instanceof Error) {
              console.error(error);
            }
          } finally {
            state.blob.setupPending = false;
          }
        }

        function syncSelection() {
          if (!tablePanel) {
            return;
          }

          const currentRows = rows();
          if (!currentRows.length) {
            state.focusedRowIndex = 0;
            delete tablePanel.dataset.mode;
            clearSelectionAttributes();
            syncBlobMode();
            return;
          }

          state.focusedRowIndex = clamp(state.focusedRowIndex, 0, currentRows.length - 1);

          clearSelectionAttributes();
          syncModeSelect();

          if (state.nerdMode) {
            tablePanel.dataset.mode = "helix";
          } else {
            delete tablePanel.dataset.mode;
          }

          currentRows.forEach((row, rowIndex) => {
            row.dataset.rowIndex = String(rowIndex);
            if (state.nerdMode && rowIndex === state.focusedRowIndex) {
              row.dataset.rowFocused = "true";
              row.scrollIntoView({ block: "nearest", inline: "nearest" });
            }
          });

          syncBlobMode();
        }

        function setNerdMode(enabled) {
          state.nerdMode = enabled === true;
          writeSettings();
          syncSelection();
        }

        function moveFocus(rowDelta) {
          const currentRows = rows();
          if (!currentRows.length) {
            return;
          }

          if (rowDelta !== 0) {
            state.focusedRowIndex = clamp(
              state.focusedRowIndex + rowDelta,
              0,
              currentRows.length - 1,
            );
          }

          syncSelection();
        }

        async function patchRecordQuantity(recordId, quantity, options = {}) {
          if (!Number.isFinite(recordId) || !Number.isFinite(quantity)) {
            return;
          }

          const row = findRowByRecordId(recordId);
          const fromQuantity = Number.isFinite(options.fromQuantity)
            ? options.fromQuantity
            : readRowQuantity(row);

          try {
            const response = await fetch(
              "/host/integrations/servers/" +
                encodeURIComponent(serverId) +
                "/table/record/" +
                encodeURIComponent(String(recordId)),
              {
                method: "PATCH",
                headers: {
                  "Content-Type": "application/json",
                },
                body: JSON.stringify({ quantity }),
              },
            );

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              setStatus("Bloqueado", "error");
              return;
            }

            if (!response.ok) {
              const raw = await response.text().catch(() => "");
              throw new Error(raw || "Nao foi possivel atualizar o record.");
            }

            applyRowQuantity(row, quantity);

            if (options.action === "record" && Number.isFinite(fromQuantity) && fromQuantity !== quantity) {
              pushQuantityHistory({
                recordId,
                from: fromQuantity,
                to: quantity,
              });
            } else if (options.action === "undo") {
              state.quantityHistoryCursor = Math.max(-1, options.historyIndex - 1);
            } else if (options.action === "redo") {
              state.quantityHistoryCursor = Math.min(
                state.quantityHistory.length - 1,
                options.historyIndex,
              );
            }

            syncSelection();
            return true;
          } catch (error) {
            if (error instanceof Error) {
              console.error(error);
            }

            return false;
          }
        }

        async function zeroFocusedQuantity() {
          const recordId = focusedRecordId();
          if (!Number.isFinite(recordId)) {
            return false;
          }

          const row = focusedRow();
          const fromQuantity = readRowQuantity(row);
          const isLastVisibleRow = rows().length === 1;
          const completionOrigin = isLastVisibleRow ? blobPointForRow(row) : null;
          const updated = await patchRecordQuantity(recordId, 0, {
            action: "record",
            fromQuantity,
          });

          if (updated && isLastVisibleRow) {
            if (row?.isConnected) {
              row.dataset.rowDispersing = "true";
            }
            startCompletionBlob(completionOrigin);
          }

          return updated;
        }

        async function applyQuantityHistoryStep(direction) {
          const entryIndex =
            direction < 0 ? state.quantityHistoryCursor : state.quantityHistoryCursor + 1;
          const entry = state.quantityHistory[entryIndex];
          if (!entry) {
            return false;
          }

          return patchRecordQuantity(entry.recordId, direction < 0 ? entry.from : entry.to, {
            action: direction < 0 ? "undo" : "redo",
            historyIndex: entryIndex,
            fromQuantity: direction < 0 ? entry.to : entry.from,
          });
        }

        function handleTableKeydown(event) {
          if (!tablePanel || !state.nerdMode) {
            return;
          }

          if (event.metaKey || event.ctrlKey || event.altKey) {
            return;
          }

          const key = event.key;
          if (key === "j" || key === "ArrowDown") {
            event.preventDefault();
            moveFocus(1);
            return;
          }

          if (key === "k" || key === "ArrowUp") {
            event.preventDefault();
            moveFocus(-1);
            return;
          }

          if (key === "u" && !event.shiftKey) {
            event.preventDefault();
            event.stopPropagation();
            void applyQuantityHistoryStep(-1);
            return;
          }

          if (key === "U" || (key === "u" && event.shiftKey)) {
            event.preventDefault();
            event.stopPropagation();
            void applyQuantityHistoryStep(1);
            return;
          }

          const isSpace =
            key === " " ||
            key === "Spacebar" ||
            key === "Space" ||
            event.code === "Space";

          if (isSpace) {
            event.preventDefault();
            event.stopPropagation();
            void zeroFocusedQuantity();
          }
        }

        function clearStream() {
          stopReconnectTimer();
          setScrolling(false);

          if (state.controller) {
            state.controller.abort();
            state.controller = null;
          }
        }

        function scheduleReconnect() {
          stopReconnectTimer();

          if (!state.streamUrl) {
            return;
          }

          const delay = Math.min(12000, 1200 * Math.max(1, state.reconnectAttempt + 1));
          state.reconnectAttempt += 1;
          state.reconnectTimer = window.setTimeout(() => connectStream(false), delay);
        }

        async function connectStream(reset) {
          clearStream();

          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          if (reset) {
            setStatus("Connecting", "loading");
          }

          const generation = ++state.streamGeneration;
          const controller = new AbortController();
          state.controller = controller;

          try {
            if (bootstrap) {
              bootstrap.dataset.streamUrl = state.streamUrl;
            }

            const response = await fetch(state.streamUrl, {
              headers: {
                Accept: "text/event-stream",
              },
              cache: "no-store",
              signal: controller.signal,
            });

            if (controller.signal.aborted || generation !== state.streamGeneration) {
              return;
            }

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              setStatus("Bloqueado", "error");
              return;
            }

            if (!response.ok || !response.body) {
              const raw = await response.text().catch(() => "");
              throw new Error(raw || `Nao foi possivel abrir o stream (${response.status}).`);
            }

            state.reconnectAttempt = 0;
            setStatus("Live", "live");

            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            let buffer = "";

            while (true) {
              const { value, done } = await reader.read();
              if (done || controller.signal.aborted || generation !== state.streamGeneration) {
                break;
              }

              buffer += decoder.decode(value, { stream: true });
              const blocks = buffer.split("\n\n");
              buffer = blocks.pop() || "";

              for (const block of blocks) {
                const trimmed = block.trim();
                if (!trimmed) {
                  continue;
                }

                const event = parseEventBlock(trimmed);
                if (!event.data || !event.event.startsWith("datastar-")) {
                  continue;
                }

                const payload = parseSsePayload(event.data);
                if (payload && typeof payload === "object") {
                  dispatchDatastarEvent(event.event, payload);
                }
              }
            }

            if (controller.signal.aborted || generation !== state.streamGeneration) {
              return;
            }

            setStatus("Reconnecting", "loading");
            scheduleReconnect();
          } catch (error) {
            if (controller.signal.aborted || generation !== state.streamGeneration) {
              return;
            }

            setStatus("Offline", "error");
            scheduleReconnect();
            if (error instanceof Error) {
              console.error(error);
            }
          } finally {
            if (state.controller === controller) {
              state.controller = null;
            }
          }
        }

        window.TodoWidget = {
          reconnect() {
            state.streamUrl = buildStreamUrl();
            if (!state.streamUrl) {
              setStatus("Configurar", "idle");
              return;
            }
            state.reconnectAttempt = 0;
            connectStream(true);
          },
        };

        if (tablePanel) {
          tablePanel.tabIndex = 0;
          tablePanel.addEventListener("keydown", handleTableKeydown);
          tablePanel.addEventListener("scroll", () => setScrolling(true), { passive: true });
          tablePanel.addEventListener("pointerdown", () => {
            if (!state.nerdMode) {
              return;
            }
            window.requestAnimationFrame(() => {
              tablePanel.focus({ preventScroll: true });
            });
          });
          tablePanel.addEventListener("focus", () => {
            if (state.nerdMode) {
              syncSelection();
            }
          });

          const observer = new MutationObserver(() => {
            syncSelection();
          });
          observer.observe(tablePanel, { childList: true, subtree: true });
        }

        if (appRoot) {
          appRoot.addEventListener("pointermove", () => {
            setChromeVisible(true);
          });
          appRoot.addEventListener("pointerleave", () => {
            setChromeVisible(false);
          });
        }

        const storedSettings = readSettings();
        if (storedSettings) {
          state.nerdMode = storedSettings.nerdMode;
        }

        const storedBlobSettings = readBlobSettings();
        if (storedBlobSettings) {
          state.blobSettings = storedBlobSettings;
        }

        syncModeSelect();
        syncBlobControls();
        renderBlobPalette();
        syncBlobMode();

        if (blobEnabledInput instanceof HTMLInputElement) {
          blobEnabledInput.addEventListener("change", () => {
            state.blobSettings.enabled = blobEnabledInput.checked;
            writeBlobSettings();
            syncSelection();
          });
        }

        if (blobViscosityInput instanceof HTMLInputElement) {
          blobViscosityInput.addEventListener("input", () => {
            state.blobSettings.viscosity = clamp(Number(blobViscosityInput.value) / 100, 0, 1);
            writeBlobSettings();
          });
        }

        if (blobEnergyInput instanceof HTMLInputElement) {
          blobEnergyInput.addEventListener("input", () => {
            state.blobSettings.energy = clamp(Number(blobEnergyInput.value) / 100, 0, 1);
            writeBlobSettings();
          });
        }

        if (blobAddColorButton) {
          blobAddColorButton.addEventListener("click", () => {
            const color = normalizeHexColor(blobColorInput?.value);
            if (!color) {
              return;
            }

            const colors = state.blobSettings.colors.filter((item) => item !== color);
            colors.unshift(color);
            state.blobSettings.colors = colors.slice(0, 8);
            writeBlobSettings();
            syncBlobControls();
            renderBlobPalette();
          });
        }

        if (blobPalette) {
          blobPalette.addEventListener("click", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
              return;
            }

            const swatch = target.closest("[data-color]");
            const color = swatch instanceof HTMLElement ? swatch.dataset.color : "";
            if (!color || state.blobSettings.colors.length <= 1) {
              return;
            }

            state.blobSettings.colors = state.blobSettings.colors.filter((item) => item !== color);
            writeBlobSettings();
            syncBlobControls();
            renderBlobPalette();
          });
        }

        document.addEventListener("change", (event) => {
          const target = event.target;
          if (!(target instanceof HTMLSelectElement) || target.id !== "table-mode") {
            return;
          }

          setNerdMode(target.value === "helix");
          if (state.nerdMode && tablePanel) {
            tablePanel.focus({ preventScroll: true });
          }
        });

        datastarReady.then(() => {
          state.streamUrl = buildStreamUrl();
          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          syncSelection();
          connectStream(false);
        });

        if (tableDetails) {
          const observer = new MutationObserver(() => {
            syncModeSelect();
          });
          observer.observe(tableDetails, { childList: true, subtree: true });
        }
      })();
    "##.to_string()
}

// SPDX-License-Identifier: Apache-2.0
// Isolated replay of captured public HTTP responses plus a disposable state authority.
// A shared TCP scheduler shapes HTTP AND WebSocket traffic; no live app edits are sent.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, verify } from "node:crypto";
import { createServer as httpServer, request as httpRequest } from "node:http";
import { createServer as tcpServer, connect } from "node:net";
import { readFile, mkdir, mkdtemp, writeFile, rm } from "node:fs/promises";
import { join, resolve, dirname } from "node:path";
import { tmpdir } from "node:os";
import {
  brotliDecompressSync,
  brotliCompressSync,
  gunzipSync,
} from "node:zlib";
const args = process.argv.slice(2),
  arg = (key, fallback) => {
    const i = args.indexOf(key);
    return i < 0 ? fallback : args[i + 1];
  };
if (args.includes("--help")) {
  console.log(
    "node scripts/profile-dashboard-browser.mjs --snapshot snapshot/snapshot.json --publisher-key publisher/public.pem --upstream http://127.0.0.1:18936 --port 18937 --network fast4g --runs 5 --output target/browser-profile [--startup-only] [--variant baseline|parallel|shared-module|combined]",
  );
  process.exit(0);
}
const publisherKey = arg("--publisher-key");
if (!publisherKey) throw Error("Supply the trusted --publisher-key public PEM");
const snapshotPath = arg("--snapshot"),
  upstream = new URL(arg("--upstream", "http://127.0.0.1:18936"));
if (!snapshotPath)
  throw Error(
    "Supply --snapshot snapshot.json and --upstream a DISPOSABLE authority",
  );
if (
  !["127.0.0.1", "localhost"].includes(upstream.hostname) ||
  upstream.protocol !== "http:"
)
  throw Error("Use a loopback disposable authority");
const variant = arg("--variant", "baseline"),
  profile = arg("--network", "fast4g"),
  runs = Number(arg("--runs", "5"));
assert(["baseline", "parallel", "shared-module", "combined"].includes(variant));
assert(["fast4g", "none"].includes(profile));
assert(Number.isInteger(runs) && runs > 0 && runs <= 30);
const output = resolve(arg("--output", "target/browser-profile"));
await mkdir(output, { recursive: true });
const snapshot = JSON.parse(await readFile(snapshotPath, "utf8")),
  assets = new Map();
assert.equal(snapshot.format, "otrios-public-snapshot-v1");
assert.match(snapshot.release.id, /^[a-f0-9]{64}$/);
const decode = (bytes, encoding) =>
  encoding === "br"
    ? brotliDecompressSync(bytes, { maxOutputLength: 32 * 1024 * 1024 })
    : encoding === "gzip"
      ? gunzipSync(bytes, { maxOutputLength: 32 * 1024 * 1024 })
      : bytes;
for (const entry of snapshot.responses) {
  const wire = await readFile(resolve(dirname(snapshotPath), entry.body)),
    body = decode(wire, entry.headers["content-encoding"]);
  if (entry.expected) {
    assert.equal(body.length, entry.expected.bytes);
    assert.equal(
      createHash("sha256").update(body).digest("hex"),
      entry.expected.sha256,
    );
  }
  assets.set(entry.path, { ...entry, wire });
}
function replace(source, from, to) {
  assert(source.includes(from), "Experiment source no longer matches");
  return source.replace(from, to);
}
const manifestEntry = assets.get(
  `/releases/${snapshot.release.id}/manifest.json`,
);
assert(manifestEntry, "Captured manifest missing");
const manifestBytes = decode(
  manifestEntry.wire,
  manifestEntry.headers["content-encoding"],
);
assert.equal(
  createHash("sha256").update(manifestBytes).digest("hex"),
  snapshot.release.id,
);
assert.deepEqual(JSON.parse(manifestBytes), snapshot.manifest);
const signature = assets.get(`/releases/${snapshot.release.id}/manifest.sig`);
assert(signature, "Captured signature missing");
assert(
  verify(
    null,
    manifestBytes,
    await readFile(publisherKey),
    decode(signature.wire, signature.headers["content-encoding"]),
  ),
  "Publisher signature rejected",
);
// Bind captured per-asset metadata to the signed manifest, not just the snapshot.
for (const [name, expected] of Object.entries(snapshot.manifest.assets)) {
  if (!name.startsWith("web/") && name !== "apps/catalog.flex") continue;
  const entry = assets.get(`/releases/${snapshot.release.id}/${name}`);
  assert(entry, `Captured public asset missing: ${name}`);
  assert.deepEqual(entry.expected, expected);
}
if (variant !== "baseline")
  for (const [path, entry] of assets) {
    let source;
    if (path.endsWith("/web/app.js")) {
      source = decode(entry.wire, entry.headers["content-encoding"]).toString();
      if (["parallel", "combined"].includes(variant))
        source = replace(
          source,
          "await init();const presenter=new CanvasPresentation();const [fontResource,appRelease]=await Promise.all([loadFontResource(import.meta.url),loadAppRelease(import.meta.url)]);",
          "const resources=Promise.all([loadFontResource(import.meta.url),loadAppRelease(import.meta.url)]);await init();const presenter=new CanvasPresentation();const [fontResource,appRelease]=await resources;",
        );
      if (["shared-module", "combined"].includes(variant))
        source = replace(
          source,
          "worker.postMessage({type:'start',fontBytes:",
          "worker.postMessage({type:'start',wasmModule:globalThis.__profileCompiledModule,fontBytes:",
        );
    } else if (
      path.endsWith("/web/state-worker.js") &&
      ["shared-module", "combined"].includes(variant)
    ) {
      source = decode(entry.wire, entry.headers["content-encoding"]).toString();
      source = replace(
        source,
        "await init();hostUrl=",
        "await init({module_or_path:m.wasmModule});hostUrl=",
      );
    }
    if (source) {
      entry.wire = brotliCompressSync(Buffer.from(source));
      entry.headers = { ...entry.headers, "content-encoding": "br" };
    }
  }
// Replay the edge's actual compressed bodies; dynamic public API/WS uses fresh roots.
const replay = httpServer((req, res) => {
  const url = new URL(req.url, "http://localhost"),
    path = url.pathname,
    entry = assets.get(path);
  if (path === "/__profile/calibration" && req.method === "POST") {
    let received = 0;
    req.on("data", (b) => {
      received += b.length;
      if (received > 1048576) req.destroy();
    });
    req.on("end", () => {
      res.writeHead(200, {
        "content-length": "0",
        "x-received-bytes": String(received),
        "cache-control": "no-store",
      });
      res.end();
    });
    return;
  }
  if (path === "/__profile/calibration") {
    const size = Number(url.searchParams.get("bytes") || 0);
    if (!Number.isInteger(size) || size < 0 || size > 1048576) {
      res.writeHead(400);
      res.end();
      return;
    }
    res.writeHead(200, {
      "content-length": String(size),
      "cache-control": "no-store",
      ...(url.searchParams.has("close") ? { connection: "close" } : {}),
    });
    res.end(Buffer.alloc(size));
    return;
  }
  if (entry) {
    const headers = {
      ...entry.headers,
      "content-length": String(entry.wire.length),
    };
    delete headers.age;
    delete headers.server;
    delete headers["cf-cache-status"];
    res.writeHead(entry.status, headers);
    res.end(entry.wire);
    return;
  }
  const forward = httpRequest(
    new URL(req.url, upstream),
    { method: req.method, headers: req.headers },
    (reply) => {
      res.writeHead(reply.statusCode, reply.headers);
      reply.pipe(res);
    },
  );
  forward.on("error", (e) => {
    res.writeHead(502);
    res.end(String(e));
  });
  req.pipe(forward);
});
const upgrades = new Set();
replay.on("upgrade", (req, client, head) => {
  const server = connect(Number(upstream.port), upstream.hostname);
  upgrades.add(client);
  upgrades.add(server);
  server.on("connect", () => {
    server.write(
      `${req.method} ${req.url} HTTP/${req.httpVersion}\r\n` +
        Object.entries(req.headers)
          .map(([k, v]) => `${k}: ${v}\r\n`)
          .join("") +
        "\r\n",
    );
    if (head.length) server.write(head);
    client.pipe(server);
    server.pipe(client);
  });
  for (const socket of [server, client]) {
    socket.on("error", () => {
      server.destroy();
      client.destroy();
    });
    socket.on("close", () => upgrades.delete(socket));
  }
});
await new Promise((resolve) => replay.listen(0, "127.0.0.1", resolve));
// Nominal current DevTools Fast 4G targets. Shape actual bytes at these rates,
// rather than applying DevTools' implementation-specific 0.9/2.75 adjustments.
const network =
  profile === "fast4g"
    ? { download_bps: 9_000_000, upload_bps: 1_500_000, rtt_ms: 60 }
    : { download_bps: 0, upload_bps: 0, rtt_ms: 0 };
let available = { up: 0, down: 0 };
const connections = new Set();
function forward(source, destination, direction) {
  let lastDelivery = 0;
  source.on("end", () => {
    const timer = setTimeout(
      () => {
        timers.delete(timer);
        if (!destination.destroyed) destination.end();
      },
      Math.max(0, lastDelivery - performance.now()) + 1,
    );
    timers.add(timer);
  });
  source.on("data", (buffer) => {
    for (let offset = 0; offset < buffer.length; offset += 16384) {
      const part = buffer.subarray(offset, offset + 16384),
        rate = network[direction === "down" ? "download_bps" : "upload_bps"];
      if (!rate) {
        destination.write(part);
        continue;
      }
      const start = Math.max(
        performance.now() + network.rtt_ms / 2,
        available[direction],
      );
      const end = start + (part.length * 8 * 1000) / rate;
      available[direction] = end;
      lastDelivery = end;
      const timer = setTimeout(
        () => {
          timers.delete(timer);
          if (!destination.destroyed) destination.write(part);
        },
        Math.max(0, end - performance.now()),
      );
      timers.add(timer);
    }
  });
}
const timers = new Set(),
  shaper = tcpServer({ allowHalfOpen: true }, (client) => {
    const server = connect({
      port: replay.address().port,
      host: "127.0.0.1",
      allowHalfOpen: true,
    });
    connections.add(client);
    connections.add(server);
    client.setNoDelay(true);
    server.setNoDelay(true);
    forward(client, server, "up");
    forward(server, client, "down");
    for (const socket of [client, server]) {
      socket.on("error", () => {
        client.destroy();
        server.destroy();
      });
      socket.on("close", () => connections.delete(socket));
    }
  });
await new Promise((resolve) =>
  shaper.listen(Number(arg("--port", "18937")), "127.0.0.1", resolve),
);
const origin = `http://127.0.0.1:${shaper.address().port}`;
const calibration = { rtt_samples: [] };
for (let i = 0; i < 3; i++) {
  const start = performance.now();
  await (await fetch(origin + "/__profile/calibration")).arrayBuffer();
  calibration.rtt_samples.push(performance.now() - start);
}
const calibrationStart = performance.now();
await Promise.all(
  [1, 2].map(async () => {
    await (
      await fetch(origin + "/__profile/calibration?bytes=524288")
    ).arrayBuffer();
  }),
);
calibration.aggregate_mib_ms = performance.now() - calibrationStart;
if (profile === "fast4g") {
  const expected = (1048576 * 8 * 1000) / network.download_bps + network.rtt_ms;
  assert(
    calibration.aggregate_mib_ms > expected * 0.85 &&
      calibration.aggregate_mib_ms < expected * 1.3,
    "Shared throughput calibration failed",
  );
  assert(
    Math.min(...calibration.rtt_samples) > network.rtt_ms * 0.85,
    "Latency calibration failed",
  );
}
calibration.closed_connection_bytes = (
  await (
    await fetch(origin + "/__profile/calibration?bytes=65536&close=1")
  ).arrayBuffer()
).byteLength;
assert.equal(
  calibration.closed_connection_bytes,
  65536,
  "Shaped connection closed before its queued body arrived",
);
const uploadStart = performance.now(),
  uploadReply = await fetch(origin + "/__profile/calibration", {
    method: "POST",
    body: Buffer.alloc(262144),
  });
await uploadReply.arrayBuffer();
assert.equal(Number(uploadReply.headers.get("x-received-bytes")), 262144);
calibration.upload_kib_ms = performance.now() - uploadStart;
if (profile === "fast4g") {
  const expected = (262144 * 8 * 1000) / network.upload_bps + network.rtt_ms;
  assert(
    calibration.upload_kib_ms > expected * 0.85 &&
      calibration.upload_kib_ms < expected * 1.3,
    "Upload calibration failed",
  );
}
console.log(JSON.stringify({ origin, upstream: upstream.href, calibration }));
function instrumentation() {
  const emit = (kind, data = {}) => {
    try {
      globalThis.__profileSink(
        JSON.stringify({
          kind,
          at: performance.timeOrigin + performance.now(),
          ...data,
        }),
      );
    } catch {}
  };
  let sequence = 0;
  globalThis.__profile = {
    events: [],
    pending: null,
  };
  for (const name of [
    "instantiateStreaming",
    "instantiate",
    "compileStreaming",
    "compile",
  ])
    if (typeof WebAssembly[name] === "function") {
      const original = WebAssembly[name];
      WebAssembly[name] = async function (...args) {
        const id = ++sequence;
        emit("wasm-start", { name, id, input: args[0]?.constructor?.name });
        try {
          const result = await original.apply(this, args);
          if (result?.module)
            globalThis.__profileCompiledModule = result.module;
          emit("wasm-end", { name, id });
          return result;
        } catch (e) {
          emit("wasm-error", { name, id, error: String(e) });
          throw e;
        }
      };
    }
  if (typeof document === "undefined") return;
  const originalWorker = globalThis.Worker;
  globalThis.Worker = class extends originalWorker {
    constructor(...args) {
      super(...args);
      emit("worker-created", { url: String(args[0]) });
      this.addEventListener("message", ({ data: m }) => {
        if (m.type === "frame") {
          emit("worker-frame", {
            ui: m.ui,
            path: m.path,
            epoch: m.epoch,
            controls: m.controls?.length || 0,
            geometry: !!m.vertices?.length,
          });
          if (m.ui && m.vertices?.length) globalThis.__profile.pending = m.ui;
        } else if (["release-id", "online", "transport-ready"].includes(m.type))
          emit("worker-message", { type: m.type, online: m.online });
      });
    }
    postMessage(message, ...rest) {
      if (message.type === "start")
        emit("worker-start-sent", {
          module: message.wasmModule instanceof WebAssembly.Module,
        });
      return super.postMessage(message, ...rest);
    }
  };
  for (const type of ["pointerdown", "pointerup"])
    document.addEventListener(
      type,
      (e) =>
        emit(type, {
          trusted: e.isTrusted,
          event_at: performance.timeOrigin + e.timeStamp,
        }),
      true,
    );
  const proto = WebGLRenderingContext.prototype;
  for (const name of ["drawArrays", "drawElements"]) {
    const original = proto[name];
    proto[name] = function (...args) {
      const result = original.apply(this, args);
      const p = globalThis.__profile;
      if (p.pending) {
        const ui = p.pending;
        p.pending = null;
        emit("draw-submitted", { ui });
        requestAnimationFrame(() => {
          if (document.querySelector("#revision")?.dataset.ui === ui) {
            const event = {
              ui,
              at: performance.timeOrigin + performance.now(),
              controls: document.querySelectorAll("#controls button").length,
            };
            p.events.push(event);
            emit("presented", event);
          }
        });
      }
      return result;
    };
  }
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries())
        emit("long-task", {
          start: performance.timeOrigin + e.startTime,
          duration: e.duration,
        });
    }).observe({ type: "longtask", buffered: true });
  } catch {}
  emit("document-script-start");
}
const injected = `(${instrumentation.toString()})()`;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function measure(index) {
  const directory = await mkdtemp(join(tmpdir(), "otrios-cold-profile-"));
  const browser = spawn(
    arg("--chromium", "chromium"),
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
      "--enable-unsafe-swiftshader",
      "--use-angle=swiftshader",
      "--remote-debugging-port=0",
      `--user-data-dir=${directory}`,
      "--no-first-run",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  let socket;
  const tasks = new Map(),
    events = [],
    requests = new Map(),
    workerSessions = new Map(),
    targetErrors = [];
  let serial = 0,
    main,
    epochOffset,
    debugEvaluate;
  try {
    const endpoint = await new Promise((resolve, reject) => {
      let log = "";
      const timer = setTimeout(
        () => reject(Error("Chromium startup timeout")),
        30000,
      );
      browser.stderr.on("data", (bytes) => {
        log += bytes;
        const m = log.match(/DevTools listening on (ws:\/\/[^\s]+)/);
        if (m) {
          clearTimeout(timer);
          resolve(m[1]);
        }
      });
      browser.once("exit", (code) =>
        reject(Error(`Chromium exited ${code}: ${log.slice(-500)}`)),
      );
      browser.once("error", reject);
    });
    socket = new WebSocket(endpoint);
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    const call = (method, params = {}, sessionId) =>
      new Promise((resolve, reject) => {
        const id = ++serial,
          timer = setTimeout(() => {
            tasks.delete(id);
            reject(Error(`CDP timeout ${method}`));
          }, 30000);
        tasks.set(id, { resolve, reject, timer });
        socket.send(JSON.stringify({ id, method, params, sessionId }));
      });
    const evaluate = async (expression, session = main) => {
      const r = await call(
        "Runtime.evaluate",
        { expression, returnByValue: true, awaitPromise: true },
        session,
      );
      if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails));
      return r.result.value;
    };
    debugEvaluate = evaluate;
    socket.addEventListener("message", ({ data }) => {
      const m = JSON.parse(data);
      if (m.id) {
        const task = tasks.get(m.id);
        if (task) {
          tasks.delete(m.id);
          clearTimeout(task.timer);
          if (m.error) task.reject(Error(JSON.stringify(m.error)));
          else task.resolve(m.result);
        }
        return;
      }
      const p = m.params || {},
        session = m.sessionId;
      if (m.method === "Target.attachedToTarget") {
        workerSessions.set(p.sessionId, p.targetInfo.type);
        (async () => {
          await call("Network.enable", {}, p.sessionId);
          await call("Runtime.enable", {}, p.sessionId);
          await call(
            "Runtime.addBinding",
            { name: "__profileSink" },
            p.sessionId,
          );
          await evaluate(injected, p.sessionId);
          await call("Runtime.runIfWaitingForDebugger", {}, p.sessionId);
        })().catch((e) => {
          targetErrors.push(String(e));
          call("Runtime.runIfWaitingForDebugger", {}, p.sessionId).catch(
            () => {},
          );
        });
        return;
      }
      if (m.method === "Runtime.bindingCalled" && p.name === "__profileSink") {
        try {
          events.push({
            ...JSON.parse(p.payload),
            target:
              session === main
                ? "page"
                : workerSessions.get(session) || "other",
          });
        } catch {}
        return;
      }
      if (m.method === "Network.webSocketHandshakeResponseReceived")
        events.push({
          kind: "ws-handshake",
          at: p.timestamp * 1000 + (epochOffset || 0),
          status: p.response.status,
          statusText: p.response.statusText,
        });
      if (m.method === "Network.webSocketFrameError")
        events.push({ kind: "ws-error", error: p.errorMessage });
      if (m.method === "Runtime.consoleAPICalled" && p.type === "error")
        events.push({
          kind: "console-error",
          error: p.args.map((x) => x.description || x.value).join(" "),
        });
      if (m.method === "Runtime.exceptionThrown")
        targetErrors.push(
          p.exceptionDetails?.exception?.description ||
            p.exceptionDetails?.text,
        );
      if (m.method === "Network.requestWillBeSent") {
        const key = `${session}:${p.requestId}`;
        if (p.type === "Document" && session === main)
          epochOffset = p.wallTime * 1000 - p.timestamp * 1000;
        requests.set(key, {
          key,
          url: p.request.url,
          target:
            session === main ? "page" : workerSessions.get(session) || "other",
          type: p.type,
          start: p.timestamp * 1000,
          initiator: p.initiator?.type,
        });
      } else if (m.method === "Network.responseReceived") {
        const r = requests.get(`${session}:${p.requestId}`);
        if (r)
          Object.assign(r, {
            response: p.timestamp * 1000,
            status: p.response.status,
            protocol: p.response.protocol,
            mime: p.response.mimeType,
            fromDiskCache: p.response.fromDiskCache,
            fromServiceWorker: p.response.fromServiceWorker,
            headers: p.response.headers,
            timing: p.response.timing,
          });
      } else if (m.method === "Network.requestServedFromCache") {
        const r = requests.get(`${session}:${p.requestId}`);
        if (r) r.cached = true;
      } else if (m.method === "Network.loadingFinished") {
        const r = requests.get(`${session}:${p.requestId}`);
        if (r)
          Object.assign(r, {
            end: p.timestamp * 1000,
            encodedBytes: p.encodedDataLength,
          });
      } else if (m.method === "Network.loadingFailed") {
        const r = requests.get(`${session}:${p.requestId}`);
        if (r) r.failure = p.errorText;
      } else if (
        [
          "Network.webSocketFrameSent",
          "Network.webSocketFrameReceived",
        ].includes(m.method)
      ) {
        const frame = p.response;
        if (frame.opcode === 2) {
          const b = Buffer.from(frame.payloadData, "base64");
          events.push({
            kind: m.method.endsWith("Sent") ? "ws-sent" : "ws-received",
            at: p.timestamp * 1000 + (epochOffset || 0),
            bytes: b.length,
            prefix: b.subarray(0, 12).toString("hex"),
            tag:
              b[0] === 1 && b.subarray(1, 5).toString() === "OTC4" && b[5] === 0
                ? b[11]
                : null,
          });
        } else if (frame.opcode === 1)
          events.push({
            kind: m.method.endsWith("Sent")
              ? "ws-control-sent"
              : "ws-control-received",
            at: p.timestamp * 1000 + (epochOffset || 0),
            bytes: Buffer.byteLength(frame.payloadData),
          });
      }
    });
    const { targetInfos } = await call("Target.getTargets");
    const page = targetInfos.find((t) => t.type === "page");
    ({ sessionId: main } = await call("Target.attachToTarget", {
      targetId: page.targetId,
      flatten: true,
    }));
    await call("Page.enable", {}, main);
    await call("Runtime.enable", {}, main);
    await call("Network.enable", {}, main);
    await call("Runtime.addBinding", { name: "__profileSink" }, main);
    await call(
      "Page.addScriptToEvaluateOnNewDocument",
      { source: injected },
      main,
    );
    await call(
      "Target.setAutoAttach",
      {
        autoAttach: true,
        waitForDebuggerOnStart: true,
        flatten: true,
        filter: [{ type: "worker" }, { exclude: true }],
      },
      main,
    );
    await call("Target.setAutoAttach", {
      autoAttach: true,
      waitForDebuggerOnStart: false,
      flatten: true,
      filter: [{ type: "service_worker" }, { exclude: true }],
    });
    await call(
      "Emulation.setDeviceMetricsOverride",
      { width: 800, height: 1280, deviceScaleFactor: 1, mobile: true },
      main,
    );
    await call(
      "Emulation.setTouchEmulationEnabled",
      { enabled: true, maxTouchPoints: 1 },
      main,
    );
    const wait = async (expression, ms = 30000) => {
      const start = Date.now();
      while (Date.now() - start < ms) {
        if (await evaluate(expression)) return;
        const error = await evaluate(
          "document.querySelector('#error')?.hidden===false?document.querySelector('#error').textContent:''",
        );
        if (error) throw Error(error);
        await pause(15);
      }
      throw Error(
        `Browser condition timeout: ${expression}; target errors ${targetErrors.join(";")}`,
      );
    };
    async function ready(ui, since) {
      await wait(
        `globalThis.__profile?.events.some(e=>e.ui===${JSON.stringify(ui)}&&e.at>=${since})&&document.querySelector('#revision')?.dataset.ui===${JSON.stringify(ui)}&&document.querySelectorAll('#controls button').length>0`,
      );
      return evaluate(
        `globalThis.__profile.events.find(e=>e.ui===${JSON.stringify(ui)}&&e.at>=${since})`,
      );
    }
    await call("Page.navigate", { url: origin + "/" }, main);
    await ready("app:home/ui", 0);
    await pause(250);
    const cold = await evaluate(
      "({origin:performance.timeOrigin,paint:performance.getEntriesByType('paint').map(e=>({name:e.name,start:e.startTime})),resources:performance.getEntriesByType('resource').map(e=>({name:e.name,start:e.startTime,end:e.responseEnd,transfer:e.transferSize,encoded:e.encodedBodySize,decoded:e.decodedBodySize})),events:__profile.events,cache:document.querySelector('#revision').dataset.cacheState,renderer:document.querySelector('#revision').dataset.fontRenderer})",
    );
    const coldEvents = events.slice(),
      coldRequests = [...requests.values()];
    if (index === 1) {
      const shot = await call(
        "Page.captureScreenshot",
        { format: "png" },
        main,
      );
      await writeFile(
        join(output, "cold-home.png"),
        Buffer.from(shot.data, "base64"),
      );
    }
    const navigations = [];
    async function tap(label, ui, phase) {
      const control = await evaluate(
        `(()=>{const b=[...document.querySelectorAll('#controls button')].find(b=>b.getAttribute('aria-label')===${JSON.stringify(label)});if(!b)throw Error('Missing control '+${JSON.stringify(label)});const r=b.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`,
      );
      const start = await evaluate("performance.timeOrigin+performance.now()"),
        eventIndex = events.length;
      await call(
        "Input.dispatchTouchEvent",
        {
          type: "touchStart",
          touchPoints: [{ ...control, radiusX: 1, radiusY: 1, force: 1 }],
        },
        main,
      );
      await pause(30);
      await call(
        "Input.dispatchTouchEvent",
        { type: "touchEnd", touchPoints: [] },
        main,
      );
      const presented = await ready(ui, start),
        points = events.slice(eventIndex).filter((e) => e.at <= presented.at),
        down = points.find((e) => e.kind === "pointerdown"),
        up = points.find((e) => e.kind === "pointerup");
      const sent = points.find((e) => e.kind === "ws-sent" && e.tag === 1),
        mounted = points.find((e) => e.kind === "ws-received" && e.tag === 18),
        frame = points.find(
          (e) => e.kind === "worker-frame" && e.ui === ui && e.geometry,
        ),
        draw = points.find((e) => e.kind === "draw-submitted" && e.ui === ui);
      assert(
        down?.trusted && up?.trusted,
        "Navigation measurement requires trusted pointer events",
      );
      const result = {
        label,
        ui,
        phase,
        start,
        down: down?.event_at,
        up: up?.event_at,
        input_delay_ms: down ? down.at - down.event_at : null,
        presented: presented.at,
        tap_ms: presented.at - (down?.event_at || start),
        release_ms: up ? presented.at - up.event_at : null,
        mount_sent: sent?.at,
        mounted: mounted?.at,
        worker_frame: frame?.at,
        draw: draw?.at,
        wire_sent: points
          .filter((e) => e.kind === "ws-sent")
          .reduce((n, e) => n + e.bytes, 0),
        wire_received: points
          .filter((e) => e.kind === "ws-received")
          .reduce((n, e) => n + e.bytes, 0),
        packets: points
          .filter((e) => e.kind.startsWith("ws-"))
          .map((e) => ({ kind: e.kind, at: e.at, tag: e.tag, bytes: e.bytes })),
      };
      navigations.push(result);
      await pause(180);
      return result;
    }
    if (!args.includes("--startup-only"))
      for (const phase of ["first-visit", "repeat-visit"])
        for (const [label, id] of [
          ["Component Playground", "component-playground"],
          ["Layout Lab", "layout-lab"],
          ["Input Lab", "input-lab"],
          ["Animation Lab", "animation-lab"],
          ["3D Lab", "three-d-lab"],
        ]) {
          await tap(label, `app:${id}/ui`, phase);
          await tap("Home", "app:home/ui", "return");
        }
    await wait(
      "document.querySelector('#revision').dataset.cacheState==='ready'",
      10000,
    );
    await pause(350);
    const beforeReload = Date.now();
    await call("Page.reload", {}, main);
    await ready("app:home/ui", beforeReload);
    await pause(250);
    const warm = await evaluate(
      "({origin:performance.timeOrigin,events:__profile.events,cache:document.querySelector('#revision').dataset.cacheState})",
    );
    const warmEvents = events.filter((e) => e.at >= warm.origin),
      warmRequests = [...requests.values()].filter(
        (r) => r.start + (epochOffset || 0) >= warm.origin,
      );
    const result = {
      index,
      profile: network,
      variant,
      release: snapshot.release.id,
      browser: await call("Browser.getVersion"),
      epochOffset,
      cold: { ...cold, events: coldEvents, requests: coldRequests },
      warm: { ...warm, events: warmEvents, requests: warmRequests },
      navigations,
      errors: targetErrors,
    };
    assert.equal(
      await evaluate("document.querySelector('#error').hidden"),
      true,
    );
    assert.equal(targetErrors.length, 0, targetErrors.join(";"));
    await writeFile(
      join(output, `run-${index}.json`),
      JSON.stringify(result, null, 2),
    );
    console.log(
      JSON.stringify({
        run: index,
        cold_ms:
          cold.events.find((e) => e.ui === "app:home/ui").at - cold.origin,
        warm_ms:
          warm.events.find((e) => e.ui === "app:home/ui").at - warm.origin,
        taps: navigations
          .filter((e) => e.phase !== "return")
          .map((e) => ({ app: e.label, phase: e.phase, ms: e.tap_ms })),
      }),
    );
    return result;
  } catch (error) {
    await writeFile(
      join(output, `failure-${index}.json`),
      JSON.stringify(
        {
          error: String(error),
          events,
          requests: [...requests.values()],
          targetErrors,
          dom: await debugEvaluate?.(
            "({title:document.title,error:document.querySelector('#error')?.textContent,revision:document.querySelector('#revision')?.outerHTML,profile:__profile})",
          ).catch(() => null),
        },
        null,
        2,
      ),
    );
    throw error;
  } finally {
    socket?.close();
    for (const t of tasks.values()) {
      clearTimeout(t.timer);
      t.reject(Error("Profiler closed"));
    }
    browser.kill("SIGTERM");
    await new Promise((resolve) =>
      browser.exitCode !== null ? resolve() : browser.once("exit", resolve),
    );
    await rm(directory, {
      recursive: true,
      force: true,
      maxRetries: 3,
      retryDelay: 100,
    });
  }
}
try {
  const results = [];
  for (let i = 1; i <= runs; i++) results.push(await measure(i));
  await writeFile(
    join(output, "results.json"),
    JSON.stringify(
      {
        calibration,
        method: {
          snapshot: snapshot.origin,
          network,
          variant,
          experimental: variant !== "baseline",
          cache:
            "fresh Chromium profile for each cold run; same-profile warm reload",
          cpu: "unthrottled",
          websocket_shaped: true,
          publisher_signature_verified: true,
          tap_uses_event_timestamp: true,
          transport: "HTTP/1.1 replay, no DNS/TLS/CDN delay",
          monitoring: "excluded",
          upstream: "disposable local authority",
        },
        results,
      },
      null,
      2,
    ),
  );
} finally {
  for (const timer of timers) clearTimeout(timer);
  for (const s of connections) s.destroy();
  for (const s of upgrades) s.destroy();
  await new Promise((resolve) => shaper.close(resolve));
  replay.closeAllConnections();
  await new Promise((resolve) => replay.close(resolve));
}

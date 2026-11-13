#!/usr/bin/env node
// One command starts an isolated engine session; no existing SQLite file or queue is reused.
import { spawn, spawnSync } from "node:child_process";
import { mkdir, open } from "node:fs/promises";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { randomUUID } from "node:crypto";

const root = fileURLToPath(new URL("../", import.meta.url));
const enginePort = process.env.ENGINE_PORT ?? "4000";
const apiPort = process.env.API_PORT ?? "3000";
const webPort = process.env.WEB_PORT ?? "5173";
const children = [];
const handles = [];
let stopping = false;
function signal(child, name) {
  try {
    if (process.platform !== "win32") process.kill(-child.pid, name);
    else child.kill(name);
  } catch {
    /* Already stopped. */
  }
}
function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  for (const child of children) signal(child, "SIGTERM");
  const timer = setTimeout(() => {
    for (const child of children) signal(child, "SIGKILL");
    process.exit(code);
  }, 3000);
  timer.unref();
  Promise.all(
    children.map((child) =>
      child.exitCode !== null || child.signalCode !== null
        ? Promise.resolve()
        : new Promise((resolve) => child.once("exit", resolve)),
    ),
  ).then(async () => {
    for (const handle of handles) await handle.close();
    process.exit(code);
  });
}
process.on("SIGINT", () => stop());
process.on("SIGTERM", () => stop());

async function available(port) {
  await new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", () =>
      reject(
        new Error(
          `Port ${port} is in use. Stop the existing service or override ENGINE_PORT / API_PORT / WEB_PORT.`,
        ),
      ),
    );
    server.listen(Number(port), "127.0.0.1", () => server.close(resolve));
  });
}
async function waitFor(check, label, timeout = 30000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (stopping) throw new Error("A service exited during startup.");
    try {
      if (await check()) return;
    } catch {
      /* Wait for startup. */
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`Timed out waiting for ${label}. See the session logs.`);
}
function command(binary, args) {
  const result = spawnSync(binary, args, { cwd: root, stdio: "inherit" });
  if (result.error || result.status !== 0)
    throw result.error ?? new Error(`${binary} exited with ${result.status}`);
}
try {
  if (new Set([enginePort, apiPort, webPort]).size !== 3)
    throw new Error("Engine, API, and web ports must be different.");
  for (const port of [enginePort, apiPort, webPort]) await available(port);
  command("cargo", ["build", "--workspace", "--locked"]);
  command("docker", ["compose", "up", "-d"]);
  await waitFor(
    async () =>
      (
        await fetch("http://127.0.0.1:15672/api/overview", {
          headers: {
            Authorization: `Basic ${Buffer.from("admin:pass").toString("base64")}`,
          },
          signal: AbortSignal.timeout(1000),
        })
      ).ok,
    "RabbitMQ",
  );
  const session = `web-${randomUUID()}`;
  const directory = path.join(root, ".local", session);
  await mkdir(directory, { recursive: true });
  const env = {
    ...process.env,
    APP_AMQP__CHANNEL: session,
    APP_AMQP__CONSUMER_TAG: session,
    APP_AMQP__HOST: "127.0.0.1",
    APP_AMQP__PORT: "5672",
    APP_AMQP__USERNAME: "admin",
    APP_AMQP__PASSWORD: "pass",
    APP_DATABASE__FILE: path.join(directory, "db.sqlite"),
  };
  console.log(`\nSession data and logs: ${directory}`);
  for (const [service, extra] of [
    ["engine", { APP_APPLICATION__PORT: enginePort }],
    ["message-persistor", {}],
    [
      "api-gateway",
      { APP_APPLICATION__PORT: apiPort, APP_ENGINE__PORT: enginePort },
    ],
  ]) {
    const log = await open(path.join(directory, `${service}.log`), "w");
    handles.push(log);
    const child = spawn(path.join(root, "target/debug", service), {
      cwd: root,
      env: { ...env, ...extra },
      detached: process.platform !== "win32",
      stdio: ["ignore", log.fd, log.fd],
    });
    children.push(child);
    child.on("error", (error) => {
      console.error(error);
      stop(1);
    });
    child.on("exit", (code) => {
      if (!stopping) {
        console.error(`${service} stopped (${code}). See ${directory}`);
        stop(1);
      }
    });
  }
  await waitFor(async () => {
    const response = await fetch(`http://127.0.0.1:${apiPort}/market`, {
      signal: AbortSignal.timeout(1000),
    });
    return response.ok && (await response.json()).status === "live";
  }, "the market feed");
  // An explicit demo flag places real simulated orders through the HTTP gateway.
  if (process.argv.includes("--seed")) {
    for (let level = 0; level < 12; level++) {
      for (const side of [1, 2]) {
        const response = await fetch(`http://127.0.0.1:${apiPort}/orders`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            user_id: String(100 + level),
            side,
            price: String(
              side === 1 ? 6542000 - level * 2500 : 6544500 + level * 2500,
            ),
            quantity: String(5000000 + ((level * 7 + side * 3) % 11) * 1000000),
            base_currency: "BTC",
            quote_currency: "USD",
          }),
        });
        if (response.status !== 202)
          throw new Error("Could not seed demo liquidity.");
      }
    }
    const response = await fetch(`http://127.0.0.1:${apiPort}/orders`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        user_id: "1",
        side: 1,
        price: "6544500",
        quantity: "1000000",
        base_currency: "BTC",
        quote_currency: "USD",
      }),
    });
    if (response.status !== 202) throw new Error("Could not seed a demo fill.");
    console.log(
      "Placed 24 resting orders and a crossing buy through the real HTTP API.",
    );
  }
  const web = spawn(
    "npm",
    [
      "run",
      "dev",
      "--workspace",
      "web",
      "--",
      "--port",
      webPort,
      "--strictPort",
    ],
    {
      cwd: root,
      env: { ...process.env, API_URL: `http://127.0.0.1:${apiPort}` },
      detached: process.platform !== "win32",
      stdio: "inherit",
    },
  );
  children.push(web);
  web.on("error", (error) => {
    console.error(error);
    stop(1);
  });
  web.on("exit", (code) => {
    if (!stopping) stop(code ?? 1);
  });
  console.log(
    `\nTrading workspace: http://127.0.0.1:${webPort}\nCtrl-C stops the services. RabbitMQ and session data are retained.\n`,
  );
} catch (error) {
  console.error(error.message);
  stop(1);
}

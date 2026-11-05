import { deepStrictEqual, strictEqual } from "node:assert/strict";
import { once } from "node:events";
import { createServer, request as httpRequest } from "node:http";
import { setTimeout } from "node:timers/promises";
import { runScenario, start } from "./output/Main/index.js";

const upstreamRequests = [];
const slowRequests = [];
const upstream = createServer((request, response) => {
  upstreamRequests.push(request.url);
  const [, kind, identifier] = request.url.split("/");
  const match = /^p(\d+)$/.exec(identifier);
  if (identifier === "slow") {
    slowRequests.push(once(response, "close").then(() => `${request.url} ${response.writableFinished}`));
  } else if (identifier === "broken") {
    response.writeHead(500);
    response.end("boom");
  } else if (match === null) {
    response.writeHead(404);
    response.end("{}");
  } else if (kind === "catalog") {
    const name = identifier === "p1" ? "Widget" : `Product ${match[1]}`;
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ name, price: identifier === "p1" ? 250 : 100 }));
  } else {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ available: identifier === "p1" ? 10 : 1000 }));
  }
});
await new Promise((resolve) => upstream.listen(0, "127.0.0.1", resolve));

const logs = [];
const failures = [];
const { port, stop } = await new Promise((resolve) =>
  start({
    host: "127.0.0.1",
    port: 0,
    upstream: `http://127.0.0.1:${upstream.address().port}`,
    cacheCapacity: 100,
    log: (message) => () => logs.push(message),
    onListening: (listening) => () => resolve(listening),
    onFailure: (message) => () => failures.push(message),
  })(),
);
const baseUrl = `http://127.0.0.1:${port}`;

const scenario = await new Promise((resolve, reject) =>
  runScenario({
    baseUrl,
    onResult: (lines) => () => resolve(lines),
    onFailure: (message) => () => reject(new Error(message)),
  })(),
);

let invalidJson;
try {
  JSON.parse("not json");
} catch (error) {
  invalidJson = error.message;
}

deepStrictEqual(scenario, [
  'GET /health 200 {"status":"ok"}',
  'GET /products/p1 200 {"id":"p1","name":"Widget","price":250,"stock":10}',
  'POST /orders 201 {"number":1,"product":"p1","quantity":2,"total":500}',
  'GET /orders/1 200 {"number":1,"product":"p1","quantity":2,"total":500}',
  'POST /orders 409 {"error":"out_of_stock","product":"p1","requested":5000,"available":10}',
  `POST /orders 400 ${JSON.stringify({ error: "bad_request", message: invalidJson })}`,
  'POST /orders 400 {"error":"bad_request","message":"quantity must be positive"}',
  'GET /products/missing 404 {"error":"not_found","message":"product missing not found"}',
  'GET /products/broken 502 {"error":"upstream_failed","message":"upstream status 500"}',
  'GET /orders/99 404 {"error":"not_found","message":"order 99 not found"}',
  'GET /orders/first 400 {"error":"bad_request","message":"invalid order number first"}',
  'DELETE /orders 404 {"error":"not_found","message":"route DELETE /orders not found"}',
  "GET /crash 500 Internal Server Error",
  'GET /health 200 {"status":"ok"}',
]);
deepStrictEqual(logs, [
  "GET /health 200",
  "GET /products/p1 200",
  "POST /orders 201",
  "GET /orders/1 200",
  "POST /orders 409",
  "POST /orders 400",
  "POST /orders 400",
  "GET /products/missing 404",
  "GET /products/broken 502",
  "GET /orders/99 404",
  "GET /orders/first 400",
  "DELETE /orders 404",
  "GET /health 200",
]);
deepStrictEqual(
  upstreamRequests.filter((url) => url.endsWith("/p1")).sort(),
  ["/catalog/p1", "/stock/p1"],
);

const cancelled = httpRequest(`${baseUrl}/products/slow`);
cancelled.on("error", () => {});
cancelled.end();
while (slowRequests.length < 2) {
  await setTimeout(5);
}
cancelled.destroy();
const abandoned = Promise.all(slowRequests).then((closed) => closed.sort());
deepStrictEqual(await Promise.race([abandoned, setTimeout(2000, "still open", { ref: false })]), [
  "/catalog/slow false",
  "/stock/slow false",
]);
strictEqual(logs.length, 13);

await new Promise((resolve) => stop(() => resolve())());
upstream.close();
deepStrictEqual(failures, []);

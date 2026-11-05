import { deepStrictEqual } from "node:assert/strict";
import { run } from "./output/Main/index.js";

const logs = [];
const errors = [];
console.log = (message) => logs.push(message);
console.error = (message) => errors.push(message);
await new Promise((resolve) => {
  process.once("beforeExit", resolve);
  run();
});

deepStrictEqual(errors, []);
const origin = /^POST (http:\/\/127\.0\.0\.1:\d+)\//.exec(logs[0])[1];
const port = origin.split(":")[2];
deepStrictEqual(logs, [
  `POST ${origin}/greet?from=test -> 200 application/json {"greeting":"Hello, Alice!","query":"from=test","contentType":"application/json"}`,
  `POST ${origin}/greet -> 400 text/plain missing name`,
  `GET ${origin}/missing -> 404 text/plain unknown route /missing`,
  `GET ${origin}/crash -> 500 text/plain Internal Server Error`,
  `GET ${origin}/missing -> 404 text/plain unknown route /missing`,
  "slow request sent",
  "slow request cancelled",
  "slow handler released",
  `second listen rejected (ListenError "Error: listen EADDRINUSE: address already in use 127.0.0.1:${port}")`,
  "server closed",
]);

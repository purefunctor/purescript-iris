import { deepStrictEqual, strictEqual } from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import { setTimeout } from "node:timers/promises";
import { cancelFetch } from "./output/Main/index.js";

let arrived = false;
let resumeArrival = null;
let requestClosed;
const server = createServer((request, response) => {
  requestClosed = once(response, "close").then(() => response.writableFinished);
  arrived = true;
  if (resumeArrival !== null) {
    resumeArrival();
  }
});
server.listen(0, "127.0.0.1");
await once(server, "listening");
const { port } = server.address();

const reports = [];
const reported = new Promise((resolve) => {
  cancelFetch({
    url: `http://127.0.0.1:${port}/hang`,
    onArrived: (resume) => () => {
      if (arrived) {
        resume();
      } else {
        resumeArrival = resume;
      }
    },
    report: (message) => () => {
      reports.push(message);
      resolve();
    },
  })();
});

await reported;
strictEqual(await Promise.race([requestClosed, setTimeout(2000, "still open", { ref: false })]), false);
server.close();
server.closeAllConnections();
deepStrictEqual(reports, ['failed (NetworkError "cancelled")']);

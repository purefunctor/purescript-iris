import { deepStrictEqual } from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import { fetchGreeting, greet } from "./output/Main/index.js";

const logs = [];
const errors = [];
console.log = (message) => logs.push(message);
console.error = (message) => errors.push(message);

greet();
deepStrictEqual(logs, ["Welcome, Alice", "Welcome, Bob"]);
deepStrictEqual(errors, ["Audit: Alice", "Audit: Bob"]);

const server = createServer((request, response) => {
  response.writeHead(200, { "content-type": "text/plain" });
  response.end(`hello from ${request.url}`);
});
server.listen(0, "127.0.0.1");
await once(server, "listening");
const { port } = server.address();

logs.length = 0;
errors.length = 0;
fetchGreeting(`http://127.0.0.1:${port}/greeting`)();
fetchGreeting(`http://127.0.0.1:1/unreachable`)();
while (logs.length + errors.length < 2) {
  await new Promise((resolve) => setTimeout(resolve, 10));
}
server.close();

deepStrictEqual(logs, ["200 hello from /greeting"]);
deepStrictEqual(errors, ['(NetworkError "TypeError: fetch failed")']);

import { strictEqual } from "node:assert/strict";
import { untilLimit, whileLimit } from "./output/Main/index.js";

for (const program of [untilLimit, whileLimit]) {
  let reads = 0;
  const action = program({ get limit() { reads += 1; return 3; } });
  strictEqual(reads, 0);
  strictEqual(action(), 3);
  strictEqual(reads, 1);
  strictEqual(action(), 3);
  strictEqual(reads, 2);
}

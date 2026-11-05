import assert from "node:assert/strict";
import * as program from "./output/Main/index.js";

assert.equal(program.inferredValue, 37);
assert.deepEqual(program.multipleValue, { console: 19, http: 23 });
assert.equal(program.delayedValue, 61);

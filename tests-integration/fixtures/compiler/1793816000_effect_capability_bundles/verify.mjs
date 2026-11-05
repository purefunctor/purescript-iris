import assert from "node:assert/strict";
import * as program from "./output/Main/index.js";

assert.equal(program.readingValue, 307);
assert.equal(program.inferredValue, 1113);
assert.equal(program.rankedValue, 502);
assert.equal(program.taggedValue, "tagged");

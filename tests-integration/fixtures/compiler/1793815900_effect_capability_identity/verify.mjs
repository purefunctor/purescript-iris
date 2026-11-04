import assert from "node:assert/strict";
import * as program from "./output/Main/index.js";

assert.equal(program.consoleValue, 19);
assert.equal(program.httpValue, 23);
assert.equal(program.misleadingValue, 42);
assert.equal(program.aliasedValue, 51);

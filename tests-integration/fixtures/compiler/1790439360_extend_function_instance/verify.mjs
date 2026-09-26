import assert from "node:assert/strict";
import * as Main from "./output/Main/index.js";

assert.equal(Main.test, "AB!");
assert.equal(Main["test'"], "AB!");

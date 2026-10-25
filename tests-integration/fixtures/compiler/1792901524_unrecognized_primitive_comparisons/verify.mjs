import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.partiallyAppliedEqual(1), true);
strictEqual(Main.partiallyAppliedLessThan(2), true);
strictEqual(Main.genericEqual({ eq: left => right => left.length === right.length })("ab")("cd"), true);
strictEqual(Main.genericLessThan({
  compare: left => right => left.length < right.length ? "LT" : left.length === right.length ? "EQ" : "GT",
})("abc")("de"), false);
strictEqual(Main.unitEqual(undefined)(undefined), true);

const wrapped = value => ({ tag: "Wrapped", _1: value });
strictEqual(Main.wrappedEqual(wrapped(1))(wrapped(2)), true);
strictEqual(Main.wrappedGreaterThan(wrapped(1))(wrapped(2)), true);
strictEqual(Main.lookalikeLessThan(1)(2), false);

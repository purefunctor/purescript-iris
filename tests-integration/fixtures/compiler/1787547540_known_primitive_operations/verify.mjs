import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.booleanNot(true), false);
strictEqual(Main.booleanConj(true)(false), false);
strictEqual(Main.booleanDisj(false)(true), true);
strictEqual(Main.partiallyAppliedConj(false), false);
strictEqual(Main.genericConj({ conj: left => right => left && right })(true)(false), false);

for (const [name, left, right, expected, calls] of [
  ["conjOrder", false, true, false, ["left"]],
  ["conjOrder", true, false, false, ["left", "right"]],
  ["disjOrder", true, false, true, ["left"]],
  ["disjOrder", false, true, true, ["left", "right"]],
  ["conjCase", false, true, false, ["left"]],
  ["conjCase", true, false, false, ["left", "right"]],
  ["disjCase", true, false, true, ["left"]],
  ["disjCase", false, true, true, ["left", "right"]],
  ["conjFunctionOrder", false, true, false, ["function", "left"]],
  ["conjFunctionOrder", true, false, false, ["function", "left", "right"]],
  ["conjPatternFunctionOrder", false, true, false, ["function", "left"]],
  ["conjPatternFunctionOrder", true, false, false, ["function", "left", "right"]],
]) {
  Main.readTrace(true);
  strictEqual(Main[name](left)(right), expected);
  deepStrictEqual(Main.readTrace(true), calls);
}

strictEqual(Main.integerAdd(2147483647)(1), -2147483648);
strictEqual(Main.integerSubtract(-2147483647)(2), 2147483647);
strictEqual(Main.integerMultiply(65536)(65536), 0);
strictEqual(Main.integerNegate(-2147483648), -2147483648);
strictEqual(Main.integerNegateLiteral, -20);
strictEqual(Main.inlineIntegerNegateLiteral, -20);
strictEqual(Main.numberNegate(20.5), -20.5);
strictEqual(Main.numberNegateLiteral, -20.5);
strictEqual(Main.numberNegateZero, -0);
strictEqual(Main.numberNegateNegativeZero, 0);
strictEqual(Main.syntacticNegativeZero, -0);
strictEqual(1 / Main.syntacticNegativeZero, -Infinity);
strictEqual(Main.numberNegate(0), -0);
strictEqual(Main.numberNegate(-0), 0);
for (const value of [Infinity, -Infinity, NaN, Number.MIN_VALUE]) {
  strictEqual(Main.numberNegate(value), -value);
}
strictEqual(Main.partiallyAppliedNegate(20), -20);
strictEqual(Main.partiallyAppliedAdd(41), 42);
strictEqual(Main.lookalikeAdd(20)(22), 42);
strictEqual(Main.lookalikeNegate(20), 20);
strictEqual(Main.genericNegate({
  Semiring0: () => ({ zero: 100 }),
  sub: left => right => left - right,
})(20), 80);

Main.readTrace(true);
strictEqual(Main.integerAddOrder(false), 42);
deepStrictEqual(Main.readTrace(true), ["left", "right"]);

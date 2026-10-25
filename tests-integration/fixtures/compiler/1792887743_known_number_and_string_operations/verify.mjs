import { deepStrictEqual, ok, strictEqual } from "node:assert/strict";
import * as Data_EuclideanRing from "./output/Data.EuclideanRing/index.js";
import * as Data_Ring from "./output/Data.Ring/index.js";
import * as Data_Semigroup from "./output/Data.Semigroup/index.js";
import * as Data_Semiring from "./output/Data.Semiring/index.js";
import * as Main from "./output/Main/index.js";

const numbers = [NaN, -Infinity, -1e16, -1.5, -0, 0, 0.1, 1, 1e16, Infinity];
const strings = ["", "a", "\uD83D", "\uDE00", "1"];

const sameValue = (actual, expected, message) => ok(Object.is(actual, expected), `${message}: ${actual} !== ${expected}`);

for (const [name, prelude] of [
  ["numberAdd", Data_Semiring.add(Data_Semiring.semiringNumber)],
  ["numberSubtract", Data_Ring.sub(Data_Ring.ringNumber)],
  ["numberMultiply", Data_Semiring.mul(Data_Semiring.semiringNumber)],
  ["numberDivide", Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingNumber)],
]) {
  for (const left of numbers) {
    for (const right of numbers) {
      sameValue(Main[name](left)(right), prelude(left)(right), `${name} ${left} ${right}`);
    }
  }
}

const append = Data_Semigroup.append(Data_Semigroup.semigroupString);
for (const left of strings) {
  for (const right of strings) {
    strictEqual(Main.stringAppend(left)(right), append(left)(right));
  }
}

sameValue(Main.numberSubtract(-0)(0), -0, "numberSubtract -0 0");
sameValue(Main.numberMultiply(-1)(0), -0, "numberMultiply -1 0");
strictEqual(Main.numberDivide(-1)(0), -Infinity);
strictEqual(Main.numberSubtractGrouped(10)(4)(1), 7);
strictEqual(Main.numberAddGrouped(1e16)(-1e16)(1), 0);
strictEqual(Main.stringAppendGrouped("a")("b")("c"), "abc");

Main.readTrace(true);
strictEqual(Main.numberAddOrder(false), 42);
deepStrictEqual(Main.readTrace(true), ["left", "right"]);

strictEqual(Main.partiallyAppliedDivide(8), 0.125);
strictEqual(Main.partiallyAppliedAppend("-suffix"), "prefix-suffix");
strictEqual(Main.integerDivide(-7)(2), -4);
strictEqual(Main.integerDivide(7)(0), 0);
deepStrictEqual(Main.arrayAppend([1])([2, 3]), [1, 2, 3]);
strictEqual(Main.genericAppend({ append: left => right => left * right })(6)(7), 42);

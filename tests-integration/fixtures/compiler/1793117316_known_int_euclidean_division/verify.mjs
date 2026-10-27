import { ok, strictEqual } from "node:assert/strict";
import * as Data_EuclideanRing from "./output/Data.EuclideanRing/index.js";
import * as Main from "./output/Main/index.js";

const integers = [-2147483648, -7, -2, -1, 0, 1, 2, 3, 7, 2147483647];

const sameValue = (actual, expected, message) => ok(Object.is(actual, expected), `${message}: ${actual} !== ${expected}`);

for (const [name, library] of [
  ["integerDivide", Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingInt)],
  ["integerModulo", Data_EuclideanRing.mod(Data_EuclideanRing.euclideanRingInt)],
]) {
  for (const left of integers) {
    for (const right of integers) {
      sameValue(Main[name](left)(right), library(left)(right), `${name} ${left} ${right}`);
    }
  }
}

strictEqual(Main.integerDivide(-7)(2), -4);
strictEqual(Main.integerModulo(-7)(2), 1);
strictEqual(Main.integerDivide(7)(-2), -3);
strictEqual(Main.integerModulo(7)(-2), 1);
strictEqual(Main.integerDivide(7)(0), 0);
strictEqual(Main.integerModulo(7)(0), 0);
strictEqual(Main.halveAndRemainder(-7), -3);
strictEqual(Main.partiallyAppliedDivide(-30), -3);

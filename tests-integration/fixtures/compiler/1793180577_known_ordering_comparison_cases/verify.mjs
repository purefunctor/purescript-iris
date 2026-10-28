import { strictEqual } from "node:assert/strict";
import * as Data_Ord from "./output/Data.Ord/index.js";
import * as Main from "./output/Main/index.js";

const names = { LT: "less", EQ: "equal", GT: "greater" };
const library = (instance) => (left, right) => names[Data_Ord.compare(instance)(left)(right)];

const integers = [-2147483648, -1, 0, 1, 2147483647];
for (const left of integers) {
  for (const right of integers) {
    strictEqual(Main.compareIntegers(left)(right), library(Data_Ord.ordInt)(left, right));
    strictEqual(Main.computedIsLess(left)(right), library(Data_Ord.ordInt)(left + 1 | 0, right * 2 | 0) === "less");
  }
}

const strings = ["", "a", "b", "ab"];
for (const left of strings) {
  for (const right of strings) {
    strictEqual(Main.compareStrings(left)(right), library(Data_Ord.ordString)(left, right));
  }
}

const numbers = [-Infinity, -1.5, -0, 0, 1.5, Infinity, NaN];
for (const left of numbers) {
  for (const right of numbers) {
    const expected = library(Data_Ord.ordNumber)(left, right);
    strictEqual(Main.compareNumbers(left)(right), expected, `compareNumbers ${left} ${right}`);
    strictEqual(Main.numberIsGreater(left)(right), expected === "greater", `numberIsGreater ${left} ${right}`);
  }
}
strictEqual(Main.compareNumbers(NaN)(1), "greater");
strictEqual(Main.numberIsGreater(1)(NaN), true);

strictEqual(Main.arrayIsLess([1])([1, 2]), true);
strictEqual(Main.arrayIsLess([2])([1, 2]), false);

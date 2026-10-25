import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Data_Eq from "./output/Data.Eq/index.js";
import * as Data_Ord from "./output/Data.Ord/index.js";
import * as Main from "./output/Main/index.js";

const booleans = [false, true];
const integers = [-2147483648, -1, 0, 1, 2147483647];
const numbers = [NaN, -Infinity, -1.5, -0, 0, 1.5, Infinity];
const chars = ["a", "b", "Z", "\uD800", "\uFFFF"];
const strings = ["", "a", "ab", "b", "Z", "\uD83D\uDE00", "\uFFFF"];

const eq = dictionary => Data_Eq.eq(dictionary);
const notEq = dictionary => Data_Eq.notEq(dictionary);
const lessThan = dictionary => Data_Ord.lessThan(dictionary);
const lessThanOrEq = dictionary => Data_Ord.lessThanOrEq(dictionary);
const greaterThan = dictionary => Data_Ord.greaterThan(dictionary);
const greaterThanOrEq = dictionary => Data_Ord.greaterThanOrEq(dictionary);

for (const [name, values, prelude] of [
  ["booleanEqual", booleans, eq(Data_Eq.eqBoolean)],
  ["integerEqual", integers, eq(Data_Eq.eqInt)],
  ["numberEqual", numbers, eq(Data_Eq.eqNumber)],
  ["charNotEqual", chars, notEq(Data_Eq.eqChar)],
  ["stringNotEqual", strings, notEq(Data_Eq.eqString)],
  ["numberNotEqual", numbers, notEq(Data_Eq.eqNumber)],
  ["booleanLessThan", booleans, lessThan(Data_Ord.ordBoolean)],
  ["integerLessThanOrEqual", integers, lessThanOrEq(Data_Ord.ordInt)],
  ["charGreaterThan", chars, greaterThan(Data_Ord.ordChar)],
  ["stringGreaterThanOrEqual", strings, greaterThanOrEq(Data_Ord.ordString)],
  ["numberLessThan", numbers, lessThan(Data_Ord.ordNumber)],
  ["numberLessThanOrEqual", numbers, lessThanOrEq(Data_Ord.ordNumber)],
  ["numberGreaterThan", numbers, greaterThan(Data_Ord.ordNumber)],
  ["numberGreaterThanOrEqual", numbers, greaterThanOrEq(Data_Ord.ordNumber)],
  ["numberNotGreaterThan", numbers, left => right => !greaterThan(Data_Ord.ordNumber)(left)(right)],
]) {
  for (const left of values) {
    for (const right of values) {
      strictEqual(Main[name](left)(right), prelude(left)(right), `${name} ${String(left)} ${String(right)}`);
    }
  }
}

strictEqual(Main.numberEqual(0)(-0), true);
strictEqual(Main.numberEqual(NaN)(NaN), false);
strictEqual(Main.numberNotEqual(NaN)(NaN), true);
strictEqual(Main.numberLessThan(NaN)(1), false);
strictEqual(Main.numberLessThanOrEqual(NaN)(1), false);
strictEqual(Main.numberGreaterThan(NaN)(1), true);
strictEqual(Main.numberGreaterThanOrEqual(NaN)(1), true);
strictEqual(Main.numberGreaterThan(1)(NaN), true);
strictEqual(Main.numberGreaterThan(-0)(0), false);
strictEqual(Main.numberGreaterThanOrEqual(-0)(0), true);

strictEqual(Main.integerBetween(0)(10)(0), true);
strictEqual(Main.integerBetween(0)(10)(10), false);
strictEqual(Main.integerBetween(0)(10)(-1), false);

Main.readTrace(true);
strictEqual(Main.comparisonOrder(false), true);
deepStrictEqual(Main.readTrace(true), ["left", "right"]);

for (const [left, flag, expected] of [[NaN, true, true], [1.5, true, true], [1.5, false, false]]) {
  Main.readTrace(true);
  strictEqual(Main.comparisonCaseOrder(left)(flag), expected);
  deepStrictEqual(Main.readTrace(true), ["left", "flag", "right"]);
}

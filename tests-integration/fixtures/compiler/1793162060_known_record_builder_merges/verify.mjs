import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import * as Builder from "./output/Record.Builder/foreign.js";
import { unsafeUnionFn } from "./output/Record.Unsafe.Union/index.js";

const same = (actual, expected) => {
  deepStrictEqual(actual, expected);
  deepStrictEqual(Object.keys(actual), Object.keys(expected));
};

const merge = left => copy => unsafeUnionFn(left, copy);
const build = (record, ...steps) => steps.reduce((copy, step) => step(copy), Builder.copyRecord(record));

const record = { x: 1, y: "y" };
const partial = { y: "y" };

same(Main.merge(record), build(record, merge({ x: 0, z: true })));
same(Main.union(partial), build(partial, merge({ z: true })));
same(Main.disjointUnion(partial), build(partial, merge({ z: true })));
same(Main.records({ x: 1 })(partial), build(partial, merge({ x: 1 })));
same(Main.literals, build({ y: "y", x: 1 }, merge({ x: 0 })));
same(Main.insertThenMerge(partial), build(partial, Builder.unsafeInsert("z")(true), merge({ x: 0 })));
deepStrictEqual(record, { x: 1, y: "y" });
deepStrictEqual(partial, { y: "y" });

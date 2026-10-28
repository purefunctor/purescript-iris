import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { unsafeUnionFn } from "./output/Record.Unsafe.Union/index.js";

const same = (actual, left, right) => {
  const expected = unsafeUnionFn(left, right);
  deepStrictEqual(actual, expected);
  deepStrictEqual(Object.keys(actual), Object.keys(expected));
};

const options = { y: "options" };
const record = { y: "record", x: 5 };

same(Main.withDefaults(options), options, { x: 1, y: "default" });
same(Main.override(record), { x: 0 }, record);
same(Main.union(options), options, { x: 1 });
same(Main.disjointUnion(options), { x: 1 }, options);
same(Main.literals, { x: 1 }, { y: "right", x: 2 });
deepStrictEqual(options, { y: "options" });
deepStrictEqual(record, { y: "record", x: 5 });

Main.readTrace(true);
same(Main.withDefaultsOrder(options), options, { x: 1, y: "default" });
deepStrictEqual(Main.readTrace(true), ["options", "default"]);

same(Main.overrideOrder(record), { x: 0 }, record);
deepStrictEqual(Main.readTrace(true), ["override", "record"]);

same(Main.literalsOrder(false), { x: 1 }, { y: "right", x: 2 });
deepStrictEqual(Main.readTrace(true), ["left", "right"]);

same(Main.overrideBranchOrder(record), { y: "y", x: 2 }, record);
deepStrictEqual(Main.readTrace(true), ["override", "fallback"]);

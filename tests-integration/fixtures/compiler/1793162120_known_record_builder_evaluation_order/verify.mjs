import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const same = (actual, expected) => {
  deepStrictEqual(actual, expected);
  deepStrictEqual(Object.keys(actual), Object.keys(expected));
};

Main.readTrace(true);
same(Main.flipped({ x: 1 }), { x: 2, y: "y", z: true });
deepStrictEqual(Main.readTrace(true), ["y", "modify", "z", "record", "apply"]);

same(Main.composed({ x: 1 }), { x: 2, y: "y" });
deepStrictEqual(Main.readTrace(true), ["y", "record", "apply"]);

same(Main.literal(false), { x: 2, y: 3 });
deepStrictEqual(Main.readTrace(true), ["x", "y", "z", "first", "second"]);

import { deepStrictEqual, notStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const first = Main.fresh();
const second = Main.fresh();
notStrictEqual(first, second);
first.value = 3;
strictEqual(second.value, 0);

deepStrictEqual(Main.pair(), { same: false, left: 1, right: 0 });

const allocated = Main.allocated(4)();
strictEqual(allocated.length, 4);
strictEqual(new Set(allocated).size, 4);
deepStrictEqual(allocated.map((reference) => reference.value), [0, 1, 2, 3]);

const reference = Main.fresh();
strictEqual(Main.throughCallback(reference)((target) => () => { target.value = 42; })(), 42);
strictEqual(Main.throughCallback(reference)(Main.incrementTwice)(), 44);
strictEqual(reference.value, 44);

strictEqual(Main.throughForeign(), 20);
deepStrictEqual(Main.shared(), { first: 5, second: 6 });

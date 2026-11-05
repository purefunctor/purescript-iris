import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { marks } from "./output/Main/foreign.js";

const counter = Main.fresh();

const forCaptured = Main.forCaptured(counter);
deepStrictEqual(marks.splice(0), ["high", "callback"]);
forCaptured();
strictEqual(counter.value, 3);
forCaptured();
strictEqual(counter.value, 6);
deepStrictEqual(marks.splice(0), []);

const forInlineBody = Main.forInlineBody(counter);
deepStrictEqual(marks.splice(0), []);
forInlineBody();
strictEqual(counter.value, 2);
deepStrictEqual(marks.splice(0), ["body", "body", "body"]);

counter.value = 0;
const untilCaptured = Main.untilCaptured(counter);
deepStrictEqual(marks.splice(0), ["action"]);
untilCaptured();
strictEqual(counter.value, 2);
untilCaptured();
strictEqual(counter.value, 3);
deepStrictEqual(marks.splice(0), []);

counter.value = 0;
const whileCaptured = Main.whileCaptured(counter);
deepStrictEqual(marks.splice(0), ["condition", "action"]);
whileCaptured();
strictEqual(counter.value, 3);
whileCaptured();
strictEqual(counter.value, 3);
deepStrictEqual(marks.splice(0), []);

counter.value = 0;
const foreachCaptured = Main.foreachCaptured(counter)([1, 2, 3]);
deepStrictEqual(marks.splice(0), ["elements", "callback"]);
foreachCaptured();
strictEqual(counter.value, 6);
foreachCaptured();
strictEqual(counter.value, 12);
deepStrictEqual(marks.splice(0), []);

const partial = Main.forPartial(3);
const seen = [];
partial((index) => () => seen.push(index))();
partial((index) => () => seen.push(index * 10))();
deepStrictEqual(seen, [0, 1, 2, 0, 10, 20]);

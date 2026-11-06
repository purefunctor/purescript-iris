import { notStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const firstEffect = Main.freshEffect();
const secondEffect = Main.freshEffect();
notStrictEqual(firstEffect, secondEffect);
strictEqual(firstEffect.value, secondEffect.value);
strictEqual(firstEffect.value(), 1);

const firstFunction = Main.freshFunction();
const secondFunction = Main.freshFunction();
notStrictEqual(firstFunction, secondFunction);
strictEqual(firstFunction.value, secondFunction.value);
strictEqual(firstFunction.value(1), 2);

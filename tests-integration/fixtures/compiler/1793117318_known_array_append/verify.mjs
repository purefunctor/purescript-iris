import { deepStrictEqual, notStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.arrayAppend([1])([2, 3]), [1, 2, 3]);
deepStrictEqual(Main.arrayAppend([])([]), []);
deepStrictEqual(Main.arrayAppendChain([1])([])([2, 3]), [1, 2, 3]);
deepStrictEqual(Main.arrayAppendGrouped([1])([2])([3])([4]), [1, 2, 3, 4]);
deepStrictEqual(Main.arrayAppendLiterals([7, 8]), [1, 2, 7, 8, 3]);
deepStrictEqual(Main.literalAppend, [1, 2, 3]);
deepStrictEqual(Main.partiallyAppliedAppend([1]), [0, 1]);

const left = [1];
const right = [2];
const appended = Main.arrayAppend(left)(right);
notStrictEqual(appended, left);
notStrictEqual(appended, right);
deepStrictEqual(left, [1]);
deepStrictEqual(right, [2]);

import { deepStrictEqual } from "node:assert/strict";
import { peekImpl, pokeImpl } from "./output/Data.Array.ST/foreign.js";
import { Just, Nothing } from "./output/Data.Maybe/index.js";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace, traced } from "./output/Main/foreign.js";

const libraryModify = (index, f, xs) => {
  const entry = peekImpl(Just, Nothing, index, xs);
  return entry === Nothing ? false : pokeImpl(index, f(entry._1), xs);
};

const run = (modify, index, makeFunction) => {
  const array = [1, 2, 3];
  const calls = [];
  const f = makeFunction(array, calls);
  resetTrace();
  const modified = modify(index, f, array);
  return { modified, array, calls, trace: readTrace() };
};

const compiled = (index, f, xs) => Main.modifyAt(index)(f)(xs)();
const library = (index, f, xs) => {
  const modified = libraryModify(traced("index")(index), traced("function")(f), traced("array")(xs));
  return modified;
};

const increment = (_array, calls) => x => {
  calls.push(x);
  traced("call")(x);
  return x + 10;
};
const shrinking = (array, calls) => x => {
  calls.push(x);
  traced("call")(x);
  array.length = 0;
  return x + 10;
};

const cases = [
  [1, increment, { modified: true, array: [1, 12, 3], calls: [2], trace: ["index", "function", "array", "call"] }],
  [3, increment, { modified: false, array: [1, 2, 3], calls: [], trace: ["index", "function", "array"] }],
  [-1, increment, { modified: false, array: [1, 2, 3], calls: [], trace: ["index", "function", "array"] }],
  [1, shrinking, { modified: false, array: [], calls: [2], trace: ["index", "function", "array", "call"] }],
];

for (const [index, makeFunction, expected] of cases) {
  deepStrictEqual(run(library, index, makeFunction), expected);
  deepStrictEqual(run(compiled, index, makeFunction), expected);
}

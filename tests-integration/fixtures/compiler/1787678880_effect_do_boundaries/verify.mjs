import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

const startupTrace = readTrace();

resetTrace();
const aliased = Main.aliased("alias");
const aliasedConstruction = readTrace();
const aliasedValue = aliased();
const aliasedTrace = readTrace();

const actual = {
  startupTrace,
  aliasedConstruction,
  aliasedValue,
  aliasedTrace,
};

const expected = {
  startupTrace: ["mark:deferred-value", "construct:deferred-action"],
  aliasedConstruction: ["construct:alias-first"],
  aliasedValue: "alias",
  aliasedTrace: [
    "construct:alias-first",
    "run:alias-first",
    "construct:alias-second",
    "run:alias-second",
  ],
};

deepStrictEqual(actual, expected);

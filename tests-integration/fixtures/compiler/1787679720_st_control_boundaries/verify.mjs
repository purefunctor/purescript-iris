import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { readTrace, resetTrace } from "./output/Main/foreign.js";

const startupTrace = readTrace();

resetTrace();
const branchedThen = Main.branched(true)("then");
const branchedThenConstruction = readTrace();
const branchedThenValue = branchedThen();
const branchedThenTrace = readTrace();

resetTrace();
const branchedElse = Main.branched(false)("else");
const branchedElseConstruction = readTrace();
const branchedElseValue = branchedElse();
const branchedElseTrace = readTrace();

resetTrace();
const patternLet = Main.patternLet("pattern");
const patternLetConstruction = readTrace();
const patternLetValue = patternLet();
const patternLetTrace = readTrace();

resetTrace();
const aliased = Main.aliased("alias");
const aliasedConstruction = readTrace();
const aliasedValue = aliased();
const aliasedTrace = readTrace();

const actual = {
  startupTrace,
  branchedThenConstruction,
  branchedThenValue,
  branchedThenTrace,
  branchedElseConstruction,
  branchedElseValue,
  branchedElseTrace,
  patternLetConstruction,
  patternLetValue,
  patternLetTrace,
  aliasedConstruction,
  aliasedValue,
  aliasedTrace,
};

const expected = {
  startupTrace: ["mark:deferred-value", "construct:deferred-action"],
  branchedThenConstruction: ["construct:branch-action"],
  branchedThenValue: "then",
  branchedThenTrace: [
    "construct:branch-action",
    "run:branch-action",
    "construct:branch-then",
    "run:branch-then",
  ],
  branchedElseConstruction: ["construct:branch-action"],
  branchedElseValue: "else",
  branchedElseTrace: [
    "construct:branch-action",
    "run:branch-action",
    "construct:branch-else",
    "run:branch-else",
  ],
  patternLetConstruction: ["construct:pattern-action"],
  patternLetValue: "pattern",
  patternLetTrace: [
    "construct:pattern-action",
    "run:pattern-action",
    "construct:pattern-result",
    "run:pattern-result",
  ],
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

import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import * as Builder from "./output/Record.Builder/foreign.js";

const same = (actual, expected) => {
  deepStrictEqual(actual, expected);
  deepStrictEqual(Object.keys(actual), Object.keys(expected));
};

const build = (record, ...steps) => steps.reduce((copy, step) => step(copy), Builder.copyRecord(record));

const record = { x: 1, y: "y" };

same(Main.insertFlipped({ x: 1 }), build({ x: 1 }, Builder.unsafeInsert("y")("y"), Builder.unsafeInsert("z")(true)));
same(Main.insertComposed({ x: 1 }), build({ x: 1 }, Builder.unsafeInsert("y")("y"), Builder.unsafeInsert("z")(true)));
same(Main.modify(record), build(record, Builder.unsafeModify("x")(x => x + 1)));
same(Main.delete(record), build(record, Builder.unsafeDelete("x")));
same(Main.rename(record), build(record, Builder.unsafeRename("x")("z")));

const pipeline = [
  Builder.unsafeInsert("z")(true),
  Builder.unsafeModify("x")(x => x * 2),
  Builder.unsafeDelete("y"),
  Builder.unsafeRename("x")("w"),
];
same(Main.pipeline(record), build(record, ...pipeline));
same(Main.literal, build(record, ...pipeline));
same(Main.fromScratch, build({}, Builder.unsafeInsert("x")(1), Builder.unsafeInsert("y")("y")));
same(Main.unknown(copy => Builder.unsafeInsert("y")("y")(copy))({ x: 1 }), { x: 1, y: "y" });
deepStrictEqual(record, { x: 1, y: "y" });

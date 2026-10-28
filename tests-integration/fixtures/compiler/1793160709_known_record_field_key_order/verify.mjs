import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import * as Record from "./output/Record/index.js";

const x = { reflectSymbol: () => "x" };
const proxy = "Proxy";

const same = (actual, expected) => {
  deepStrictEqual(actual, expected);
  deepStrictEqual(Object.keys(actual), Object.keys(expected));
};

const set = (value, record) => Record.set(x)({})({})(proxy)(value)(record);
const insert = (value, record) => Record.insert(x)({})({})(proxy)(value)(record);
const modify = (f, record) => Record.modify(x)({})({})(proxy)(f)(record);
const remove = record => Record.delete(x)({})({})(proxy)(record);
const increment = value => value + 1;

same(Main.set({ y: "y", x: 0 }), set("set", { y: "y", x: 0 }));
same(Main.setLiteral("y"), set("set", { y: "y", x: 0 }));
same(Main.insert({ y: "y" }), insert(1, { y: "y" }));
same(Main.insertLiteral("y"), insert(1, { z: 2, y: "y" }));
same(Main.modify({ y: "y", x: 0 }), modify(increment, { y: "y", x: 0 }));
same(Main.modifyLiteral("y"), modify(increment, { y: "y", x: 0 }));
same(Main.delete({ z: 2, x: 0, y: "y" }), remove({ z: 2, x: 0, y: "y" }));
same(Main.deleteLiteral("y"), remove({ z: 2, x: 0, y: "y" }));

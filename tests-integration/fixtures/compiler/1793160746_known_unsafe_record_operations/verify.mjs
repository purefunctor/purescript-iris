import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import * as Unsafe from "./output/Record.Unsafe/index.js";

const same = (actual, expected) => {
  deepStrictEqual(actual, expected);
  deepStrictEqual(Object.keys(actual), Object.keys(expected));
};

const record = { x: 1, y: "y" };
const quoted = { "first name": "Ada" };
const partial = { y: "y" };

deepStrictEqual(Main.get(record), Unsafe.unsafeGet("x")(record));
deepStrictEqual(Main.getQuoted(quoted), Unsafe.unsafeGet("first name")(quoted));
same(Main.set(record), Unsafe.unsafeSet("x")(2)(record));
same(Main.setNew(partial), Unsafe.unsafeSet("x")(2)(partial));
same(Main.delete(record), Unsafe.unsafeDelete("x")(record));
deepStrictEqual(record, { x: 1, y: "y" });
deepStrictEqual(partial, { y: "y" });

Main.readTrace(true);
same(Main.deleteLiteral(false), Unsafe.unsafeDelete("x")({ x: 1, y: "y" }));
deepStrictEqual(Main.readTrace(true), ["x", "y"]);

same(Main.setOrder(record), Unsafe.unsafeSet("x")(2)(record));
deepStrictEqual(Main.readTrace(true), ["value", "record"]);

deepStrictEqual(Main.deleteOrder(record), { first: 1, rest: { y: "y" } });
deepStrictEqual(Main.readTrace(true), ["first", "record"]);

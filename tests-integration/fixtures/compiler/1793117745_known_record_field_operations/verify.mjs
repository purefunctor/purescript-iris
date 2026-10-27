import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const record = { x: 1, y: "y" };

strictEqual(Main.get(record), 1);
strictEqual(Main.getQuoted({ "first name": "Ada" }), "Ada");
deepStrictEqual(Main.set(record), { x: "set", y: "y" });
deepStrictEqual(Main.modify(record), { x: 2, y: "y" });
deepStrictEqual(Main.insert({ y: "y" }), { x: 1, y: "y" });
deepStrictEqual(record, { x: 1, y: "y" });
strictEqual(Main.getPolymorphic({ reflectSymbol: () => "y" })({})("Proxy")(record), "y");

Main.readTrace(true);
deepStrictEqual(Main.setOrder(record), { x: "set", y: "y" });
deepStrictEqual(Main.readTrace(true), ["value", "record"]);

deepStrictEqual(Main.modifyOrder(record), { x: 2, y: "y" });
deepStrictEqual(Main.readTrace(true), ["function", "record"]);

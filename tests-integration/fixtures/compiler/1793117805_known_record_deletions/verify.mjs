import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const record = { x: 1, y: "y" };

deepStrictEqual(Main.delete(record), { y: "y" });
deepStrictEqual(Main.deleteQuoted({ "first name": "Ada", y: "y" }), { y: "y" });
deepStrictEqual(record, { x: 1, y: "y" });

Main.readTrace(true);
deepStrictEqual(Main.deleteOrder(record), { first: 1, rest: { y: "y" } });
deepStrictEqual(Main.readTrace(true), ["first", "record"]);

import { deepStrictEqual, strictEqual, throws } from "node:assert/strict";
import * as Library from "./output/Library/index.js";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Object.keys(Library).sort(), [
  "Array",
  "Error",
  "Object",
  "describe",
  "objectSize",
]);
deepStrictEqual(Object.keys(Main).sort(), [
  "Array",
  "Error",
  "Object",
  "describe",
  "keys",
  "marker",
  "member",
  "objectSize",
  "sample",
  "wrap",
]);

strictEqual(Main.Object, Library.Object);
strictEqual(Main.Array, Library.Array);
strictEqual(Main.Error, Library.Error);
strictEqual(Main.wrap, Library.Object);
deepStrictEqual(Main.wrap({ a: 1 }), { tag: "Object", _1: { a: 1 } });

deepStrictEqual(Library.describe(Main.sample), ["a", "b"]);
deepStrictEqual(Library.describe(Library.Object({ b: 1 })), []);
deepStrictEqual(Library.describe(Library.Array([1, 2])), ["3"]);
deepStrictEqual(Library.describe(Library.Array([1])), []);
deepStrictEqual(Library.describe(Library.Error("message")), ["message"]);

strictEqual(Library.objectSize(Main.sample), 2);
throws(
  () => Library.objectSize(Library.Error("message")),
  (error) => error.constructor === Error && error.message === "Pattern match failure",
);

deepStrictEqual(Main.keys({ x: 1, y: 2 }), ["x", "y"]);
strictEqual(Main.member("a")({ a: 1 }), true);
strictEqual(Main.member("z")({ a: 1 }), false);
strictEqual(Main.member("constructor")({}), true);
strictEqual(Main.marker, 7);

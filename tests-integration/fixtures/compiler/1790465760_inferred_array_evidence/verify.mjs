import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

for (const functions of [Main.test({}), Main["test'"]({})]) {
  strictEqual(functions[0](Main.Chosen(17)), 17);
  strictEqual(functions[1](Main.Chosen(17)), 23);
}

const functions = Main.test2({});
strictEqual(functions[0](Main.Chosen(31)), 31);
strictEqual(functions[0](Main.Other), 41);
strictEqual(functions[1](Main.Chosen(31)), 29);

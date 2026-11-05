import { deepStrictEqual, throws } from "node:assert/strict";
import { launchDefect, run } from "./output/Main/index.js";

const reports = [];
const report = (message) => () => reports.push(message);
run(report)();
await new Promise((resolve) => setTimeout(resolve, 100));
deepStrictEqual(reports, [
  "released",
  "sibling completed",
  "await: died boom",
  "attempt: died boom",
  "cleanup over typed: died cleanup",
  "body over cleanup: died body",
  "kill with failing finalizer: died finalizer",
  "null: died null",
  "hostile: died non-error value",
  "parent continues",
]);

reports.length = 0;
throws(() => launchDefect(report)(), { message: "root" });
deepStrictEqual(reports, []);

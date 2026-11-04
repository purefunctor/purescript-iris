import { deepStrictEqual } from "node:assert/strict";
import { beforeImmediateCallback, insideCatch } from "./output/Main/index.js";

const collect = async (program) => {
  const reports = [];
  program((message) => () => reports.push(message))();
  await new Promise((resolve) => setTimeout(resolve, 50));
  return reports;
};

deepStrictEqual(await collect(insideCatch), ["before kill", "after kill", "failed self"]);
deepStrictEqual(await collect(beforeImmediateCallback), ["before kill", "after callback 5", "failed self"]);

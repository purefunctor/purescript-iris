import { deepStrictEqual } from "node:assert/strict";

const { runLowered, runGeneric } = await import("./output/Main/index.js");
const { recorded, completed, errors } = await import("./output/Main/foreign.js");

const observe = async (run) => {
  recorded.length = 0;
  completed.length = 0;
  run();
  await new Promise((resolve) => setTimeout(resolve, 10));
  return {
    recorded: [...recorded],
    completed: [...completed],
    errors: [...errors],
  };
};

const expected = {
  recorded: [4, 9, 35],
  completed: [70],
  errors: [],
};

deepStrictEqual(await observe(runLowered), expected);
deepStrictEqual(await observe(runGeneric), expected);

import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.writes([1, 2, 3]), {
  inside: true,
  past: false,
  negative: false,
  modified: true,
  unmodified: false,
  final: [40, 10, 6],
});

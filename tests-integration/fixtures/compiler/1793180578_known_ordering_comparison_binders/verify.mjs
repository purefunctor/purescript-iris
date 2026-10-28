import { strictEqual } from "node:assert/strict";
import * as Data_Ord from "./output/Data.Ord/index.js";
import * as Main from "./output/Main/index.js";

const integers = [-2147483648, -1, 0, 1, 2147483647];
for (const left of integers) {
  for (const right of integers) {
    const ordering = Data_Ord.compare(Data_Ord.ordInt)(left)(right);
    strictEqual(Main.isLess(left)(right), ordering === "LT" ? "less" : "not less");
    strictEqual(Main.describeOrdering(left)(right), ordering === "LT" ? "less" : ordering);
  }
}

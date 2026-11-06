import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const left = (message) => ({ tag: "Left", _1: message });
const right = (value) => ({ tag: "Right", _1: value });

deepStrictEqual(Main.forAborts(), { outcome: left("stop at 3"), visited: ["0", "1", "2"] });
deepStrictEqual(Main.untilAborts(), { outcome: left("stop at 2"), visited: ["1", "2"] });
deepStrictEqual(Main.whileAborts(), { outcome: left("stop at 3"), visited: ["1", "2"] });
deepStrictEqual(Main.foreachAborts(), { outcome: left("stop at c"), visited: ["a", "b"] });
deepStrictEqual(Main.nestedAborts(), {
  outcome: left("stop at 1:1"),
  visited: ["row 0", "0:0", "0:1", "0:2", "row 1", "1:0"],
});
deepStrictEqual(Main.recovers(), {
  outcome: right(undefined),
  visited: ["0", "caught one", "2", "3"],
});

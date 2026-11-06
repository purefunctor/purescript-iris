import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

const call = closures => closures.map(closure => closure(undefined));

deepStrictEqual(call(Main.rangeClosures()), [0, 1, 2]);
deepStrictEqual(call(Main.elementClosures(["a", "b", "c"])()), ["a", "b", "c"]);
deepStrictEqual(call(Main.stateClosures), [5, 6, 7]);

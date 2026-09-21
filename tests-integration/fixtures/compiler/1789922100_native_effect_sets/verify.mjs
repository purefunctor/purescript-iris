import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

deepStrictEqual(Main.combined, { tag: "Program", _1: 1 });
deepStrictEqual(Main.reordered, { tag: "Program", _1: 1 });
deepStrictEqual(Main.deduplicated, { tag: "Program", _1: 1 });
deepStrictEqual(Main.duplicateAnnotation, { tag: "Program", _1: 1 });
deepStrictEqual(Main.expanded, { tag: "Program", _1: 1 });
deepStrictEqual(Main.handled, { tag: "Program", _1: 1 });
deepStrictEqual(Main.failures, { tag: "Program", _1: 0 });
deepStrictEqual(Main.partiallyHandled, { tag: "Program", _1: 0 });
deepStrictEqual(Main.fullyHandled, { tag: "Program", _1: 0 });
deepStrictEqual(Main.openTail(Main.Program(42)), { tag: "Program", _1: 42 });

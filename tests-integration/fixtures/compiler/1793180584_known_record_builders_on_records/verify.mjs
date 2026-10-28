import { deepStrictEqual } from "node:assert/strict";
import * as Record_Builder from "./output/Record.Builder/index.js";
import * as Main from "./output/Main/index.js";

const point = { x: 1, y: 2 };
deepStrictEqual(Main.withZ(3)(point), { x: 1, y: 2, z: 3 });
deepStrictEqual(Main.shifted(point), { x: 2, y: 4 });
deepStrictEqual(Main.insertedThenModified(point), { x: 1, y: 2, z: 6 });
deepStrictEqual(Main.mergedOnto(point), { x: 1, y: 20, z: 30 });
deepStrictEqual(Main.withoutY(point), { x: 1 });
deepStrictEqual(point, { x: 1, y: 2 });

const swap = (record) => ({ x: record.y, y: record.x });
deepStrictEqual(Main.unknownBuilder(swap)(point), { x: 2, y: 1 });
deepStrictEqual(Record_Builder.build(swap)(point), { x: 2, y: 1 });

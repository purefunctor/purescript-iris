import { deepStrictEqual, rejects, strictEqual } from "node:assert/strict";

import { messages } from "./output/Console/foreign.js";
import * as Main from "./output/Main/index.js";
import {
  effectExecutions,
  rejectPendingPromise,
} from "./output/Main/foreign.js";

strictEqual(effectExecutions(), 0);
strictEqual(Main.liftedEffect(), 17);
strictEqual(effectExecutions(), 1);

strictEqual(await Main.main().promise, 42);
deepStrictEqual(messages, ["tracked console"]);
strictEqual(await Main.runFactoryAbort().promise, 5);
await rejects(Main.rejected().promise, /promise rejected/);
await rejects(Main.thrown().promise, /promise factory threw/);

const unhandledRejections = [];
const recordUnhandledRejection = (error) => unhandledRejections.push(error);
process.on("unhandledRejection", recordUnhandledRejection);
const pendingFiber = Main.pending();
pendingFiber.interrupt();
rejectPendingPromise();
await rejects(pendingFiber.promise, /interrupted/);
await new Promise((resolve) => setTimeout(resolve, 0));
process.off("unhandledRejection", recordUnhandledRejection);
deepStrictEqual(unhandledRejections, []);

import { deepStrictEqual, rejects, strictEqual } from "node:assert/strict";

import * as Main from "./output/Main/index.js";
import { events } from "./output/Main/foreign.js";
import * as Runtime from "./output/runtime.js";

strictEqual(await Main.main().promise, 42);
deepStrictEqual(events, ["first", "second"]);
strictEqual(await Main.runDeep().promise, 100000);
strictEqual(Main.syncRecovered(), 7);
strictEqual(await Main.runAbort().promise, 13);
strictEqual(await Main.runStructuralAbort().promise, 42);
strictEqual(Main.proofRetention, 17);
strictEqual(Main.wrappedProgram(), 21);

const unit = {};
const runtimeEvents = [];
const liftEvent = (event) => Runtime.asyncLift(() => {
  runtimeEvents.push(event);
  return unit;
});
const runAsync = (program) => Runtime.asyncRun(program)();

const synchronousRegistration = Runtime.asyncRegister((resume) => () => {
  resume(Runtime.asyncPure(1))();
  resume(Runtime.asyncPure(2))();
  return liftEvent("unused-canceler");
});
strictEqual(await runAsync(synchronousRegistration).promise, 1);
deepStrictEqual(runtimeEvents, []);

let resumeLateRegistration;
const lateRegistration = Runtime.asyncRegister((resume) => () => {
  resumeLateRegistration = resume;
  return liftEvent("late-canceler");
});
const lateFiber = runAsync(lateRegistration);
resumeLateRegistration(Runtime.asyncPure(3))();
resumeLateRegistration(Runtime.asyncPure(4))();
strictEqual(await lateFiber.promise, 3);

let resumeFirstRegistration;
let resumeSecondRegistration;
const chainedRegistrations = Runtime.asyncBind(
  Runtime.asyncRegister((resume) => () => {
    resumeFirstRegistration = resume;
    return Runtime.asyncPure(unit);
  }),
)(() => Runtime.asyncRegister((resume) => () => {
  resumeSecondRegistration = resume;
  return Runtime.asyncPure(unit);
}));
const chainedFiber = runAsync(chainedRegistrations);
resumeFirstRegistration(Runtime.asyncPure(unit))();
resumeFirstRegistration(Runtime.asyncPure(unit))();
strictEqual(await Promise.race([
  chainedFiber.promise.then(() => "completed"),
  Promise.resolve("pending"),
]), "pending");
resumeSecondRegistration(Runtime.asyncPure(4))();
strictEqual(await chainedFiber.promise, 4);

let recursive;
recursive = Runtime.asyncDefer(() => recursive);
const runningFiber = runAsync(recursive);
runningFiber.interrupt();
await rejects(runningFiber.promise, /interrupted/);

const pendingRegistration = Runtime.asyncRegister(() => () =>
  liftEvent("pending-canceler"));
const pendingFiber = runAsync(pendingRegistration);
deepStrictEqual(await runAsync(Runtime.asyncInterrupt(pendingFiber)).promise, unit);
await rejects(pendingFiber.promise, /interrupted/);
deepStrictEqual(runtimeEvents, ["pending-canceler"]);

const successfulBracket = Runtime.asyncBracket(Runtime.asyncPure("resource"))
  (() => liftEvent("release-success"))
  (() => Runtime.asyncBind(liftEvent("use-success"))(() => Runtime.asyncPure(5)));
strictEqual(await runAsync(successfulBracket).promise, 5);
deepStrictEqual(runtimeEvents.slice(-2), ["use-success", "release-success"]);

const abortedBracket = Runtime.asyncBracket(Runtime.asyncPure("resource"))
  (() => liftEvent("release-abort"))
  (() => Runtime.asyncAbort("Validation")(6));
const recoveredBracket = Runtime.asyncCatchAbort("Validation")(abortedBracket)
  ((error) => Runtime.asyncPure(error));
strictEqual(await runAsync(recoveredBracket).promise, 6);
strictEqual(runtimeEvents.at(-1), "release-abort");

const defectiveBracket = Runtime.asyncBracket(Runtime.asyncPure("resource"))
  (() => liftEvent("release-defect"))
  (() => Runtime.asyncLift(() => { throw new Error("use defect"); }));
await rejects(runAsync(defectiveBracket).promise, /use defect/);
strictEqual(runtimeEvents.at(-1), "release-defect");

const nestedBracket = Runtime.asyncBracket(Runtime.asyncPure("outer"))
  (() => liftEvent("release-outer"))
  (() => Runtime.asyncBracket(Runtime.asyncPure("inner"))
    (() => liftEvent("release-inner"))
    (() => Runtime.asyncPure(7)));
strictEqual(await runAsync(nestedBracket).promise, 7);
deepStrictEqual(runtimeEvents.slice(-2), ["release-inner", "release-outer"]);

const interruptedBracket = Runtime.asyncBracket(Runtime.asyncPure("resource"))
  (() => liftEvent("release-interrupted"))
  (() => Runtime.asyncRegister(() => () => liftEvent("use-canceler")));
const bracketFiber = runAsync(interruptedBracket);
deepStrictEqual(await runAsync(Runtime.asyncInterrupt(bracketFiber)).promise, unit);
await rejects(bracketFiber.promise, /interrupted/);
deepStrictEqual(runtimeEvents.slice(-2), ["use-canceler", "release-interrupted"]);

let completeRelease;
const suspendedRelease = Runtime.asyncBracket(Runtime.asyncPure("resource"))
  (() => Runtime.asyncRegister((resume) => () => {
    completeRelease = resume;
    return Runtime.asyncPure(unit);
  }))
  (() => Runtime.asyncPure(8));
const releaseFiber = runAsync(suspendedRelease);
releaseFiber.interrupt();
completeRelease(Runtime.asyncPure(unit))();
await rejects(releaseFiber.promise, /interrupted/);

let completeJoined;
const joinedTarget = runAsync(Runtime.asyncRegister((resume) => () => {
  completeJoined = resume;
  return Runtime.asyncPure(unit);
}));
const joinedFiber = runAsync(Runtime.asyncJoin(joinedTarget));
completeJoined(Runtime.asyncPure(8))();
strictEqual(await joinedTarget.promise, 8);
strictEqual(await joinedFiber.promise, 8);

let starts = 0;
const reusable = Runtime.asyncDefer(() => {
  starts += 1;
  return Runtime.asyncPure(starts);
});
const firstStart = runAsync(reusable);
const secondStart = runAsync(reusable);
deepStrictEqual(await Promise.all([firstStart.promise, secondStart.promise]), [1, 2]);

const deep = (remaining) => remaining === 0
  ? Runtime.asyncPure(100000)
  : Runtime.asyncDefer(() => deep(remaining - 1));
const fairFiber = runAsync(deep(100000));
strictEqual(await Promise.race([
  fairFiber.promise.then(() => "fiber"),
  Promise.resolve("host"),
]), "host");
strictEqual(await fairFiber.promise, 100000);

const completedFiber = runAsync(Runtime.asyncPure(unit));
await completedFiber.promise;
let joinLoop;
joinLoop = Runtime.asyncBind(Runtime.asyncJoin(completedFiber))(() => joinLoop);
const joinLoopFiber = runAsync(joinLoop);
strictEqual(await Promise.race([
  joinLoopFiber.promise.then(() => "fiber"),
  Promise.resolve("host"),
]), "host");
joinLoopFiber.interrupt();
await rejects(joinLoopFiber.promise, /interrupted/);

import * as Data_Eq from "../Data.Eq/index.js";
import * as $foreign from "./foreign.js";
import * as $effect from "../effect.js";
export function available(dictionary) {
  return dictionary.available;
}
export function availableInstance(subsetEffectNilEffectsDict) {
  return { available: 17 | 0 };
}
export function proofRetained(subsetEffectNilEffectsDict) {
  return /* @__PURE__ */ available(/* @__PURE__ */ availableInstance(subsetEffectNilEffectsDict));
}
export function deep(remaining) {
  return (total) => {
    if (/* @__PURE__ */ Data_Eq.eq(Data_Eq.eqInt)(remaining)(0 | 0)) {
      return $effect.asyncPure(total);
    } else {
      return $effect.asyncDefer(($record) => deep(remaining - (1 | 0) | 0)(total + (1 | 0) | 0));
    }
  };
}
export const mark = $foreign["mark"];
export const delayed = $foreign["delayed"];
export const proofRetention = /* @__PURE__ */ proofRetained({});
export const syncProgram = /* @__PURE__ */ $effect.syncBind({})(mark("first")(20 | 0))((first) => /* @__PURE__ */ $effect.syncBind({})(mark("second")(22 | 0))((second) => $effect.syncPure(first + second | 0)));
export const wrappedProgram = $effect.syncPure(21 | 0);
export const asyncProgram = /* @__PURE__ */ $effect.asyncBind({})($effect.asyncLift(syncProgram))((value) => /* @__PURE__ */ $effect.asyncDiscard({})($effect.asyncYield)(($record) => delayed(value)));
export const main = $effect.asyncRun(asyncProgram);
export const runDeep = $effect.asyncRun(deep(1e5 | 0)(0 | 0));
export const syncFailure = /* @__PURE__ */ $effect.syncAbort("c4:Main13:DatabaseError")(7 | 0);
export const syncRecovered = /* @__PURE__ */ $effect.syncCatchAbort("c4:Main13:DatabaseError")({})({})(syncFailure)((code) => $effect.syncPure(code));
export const asyncFailures = /* @__PURE__ */ $effect.asyncAbort("c4:Main15:ValidationError")(13 | 0);
export const asyncPartiallyHandled = /* @__PURE__ */ $effect.asyncCatchAbort("c4:Main13:DatabaseError")({})({})(asyncFailures)(($databaseError) => $effect.asyncPure(0 | 0));
export const asyncRecovered = /* @__PURE__ */ $effect.asyncCatchAbort("c4:Main15:ValidationError")({})({})(asyncPartiallyHandled)((code) => $effect.asyncPure(code));
export const runAbort = $effect.asyncRun(asyncRecovered);
export const structuralFailure = /* @__PURE__ */ $effect.asyncAbort("ac4:Prim6:Recordr2:1:ac4:Prim3:Int1:bc4:Prim6:String.")({
  a: 1 | 0,
  b: "first"
});
export const structuralRecovered = /* @__PURE__ */ $effect.asyncCatchAbort("ac4:Prim6:Recordr2:1:ac4:Prim3:Int1:bc4:Prim6:String.")({})({})(/* @__PURE__ */ $effect.asyncCatchAbort("ac4:Prim6:Recordr1:16:a :: Prim.Int, bc4:Prim6:String.")({})({})(structuralFailure)(($secondStructuralError) => $effect.asyncPure(99 | 0)))(($firstStructuralError) => $effect.asyncPure(42 | 0));
export const runStructuralAbort = $effect.asyncRun(structuralRecovered);

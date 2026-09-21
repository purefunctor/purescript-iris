import * as Console from "../Console/index.js";
import * as $foreign from "./foreign.js";
import * as $effect from "../effect.js";
export function runGeneric(runnableEffectsDict) {
  return (subsetEffectsEffectsDict) => {
    return /* @__PURE__ */ $effect.asyncRun(runnableEffectsDict);
  };
}
export const countedEffect = $foreign["countedEffect"];
export const delayedPromise = $foreign["delayedPromise"];
export const rejectedPromise = $foreign["rejectedPromise"];
export const throwingPromise = $foreign["throwingPromise"];
export const pendingPromise = $foreign["pendingPromise"];
export const liftedEffect = $effect.syncLiftEffect(countedEffect(17 | 0));
export const trackedConsole = Console.log("tracked console");
export const program = /* @__PURE__ */ $effect.asyncDiscard({})($effect.asyncLift(trackedConsole))(($unit) => /* @__PURE__ */ $effect.asyncBind({})($effect.asyncFromPromise($effect.syncLiftEffect(delayedPromise(41 | 0))))((value) => $effect.asyncPure(value + (1 | 0) | 0)));
export const main = /* @__PURE__ */ $effect.asyncRun({})(program);
export const rejected = /* @__PURE__ */ $effect.asyncRun({})($effect.asyncFromPromise($effect.syncLiftEffect(rejectedPromise)));
export const thrown = /* @__PURE__ */ $effect.asyncRun({})($effect.asyncFromPromise($effect.syncLiftEffect(throwingPromise)));
export const pending = /* @__PURE__ */ $effect.asyncRun({})($effect.asyncFromPromise($effect.syncLiftEffect(pendingPromise)));
export const factoryAbort = /* @__PURE__ */ $effect.asyncCatchAbort("c4:Main12:PromiseError")({})({})($effect.asyncFromPromise(/* @__PURE__ */ $effect.syncAbort("c4:Main12:PromiseError")(5 | 0)))((code) => $effect.asyncPure(code));
export const runFactoryAbort = /* @__PURE__ */ $effect.asyncRun({})(factoryAbort);

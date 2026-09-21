import * as runtime from "./runtime.js";

export const syncPure = runtime.syncPure;
export const syncBind = (_unionEvidence) => runtime.syncBind;
export const syncDiscard = syncBind;
export const syncMap = runtime.syncMap;
export const syncApply = (_unionEvidence) => runtime.syncApply;
export const syncAbort = runtime.syncAbort;
export const syncCatchAbort = (identity) => (_removeEvidence) => (_unionEvidence) =>
  runtime.syncCatchAbort(identity);
export const syncLiftEffect = runtime.syncLiftEffect;

export const asyncPure = runtime.asyncPure;
export const asyncBind = (_unionEvidence) => runtime.asyncBind;
export const asyncDiscard = asyncBind;
export const asyncMap = runtime.asyncMap;
export const asyncApply = (_unionEvidence) => runtime.asyncApply;
export const asyncLift = runtime.asyncLift;
export const asyncDefer = runtime.asyncDefer;
export const asyncYield = runtime.asyncYield;
export const asyncRegister = runtime.asyncRegister;
export const asyncBracket = (_acquiredEffectsEvidence) => (_combinedEffectsEvidence) =>
  runtime.asyncBracket;
export const asyncFromPromise = runtime.asyncFromPromise;
export const asyncRun = (_runnableEvidence) => runtime.asyncRun;
export const asyncJoin = runtime.asyncJoin;
export const asyncInterrupt = runtime.asyncInterrupt;
export const asyncAbort = runtime.asyncAbort;
export const asyncCatchAbort = (identity) => (_removeEvidence) => (_unionEvidence) =>
  runtime.asyncCatchAbort(identity);

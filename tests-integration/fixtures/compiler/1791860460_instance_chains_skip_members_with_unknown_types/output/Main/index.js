export function needsFallback(fallbackArrayIntDict) {
  return 0 | 0;
}
export function skipped(unavailableDict) {
  return {};
}
export function needsSkipped(skippedInvalidDict) {
  return 0 | 0;
}
export function skippedNested(unavailableDict) {
  return {};
}
export function needsSkippedNested(skippedNestedInvalidDict) {
  return 0 | 0;
}
export const fallbackArrayInvalid = {};
export const fallback = {};
export const usesFallback = /* @__PURE__ */ needsFallback(fallback);
export const skippedInvalid = {};
export const usesSkipped = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ needsSkipped(/* @__PURE__ */ skipped($result));
})();
export const skippedNestedArrayInvalid = {};
export const usesSkippedNested = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ needsSkippedNested(/* @__PURE__ */ skippedNested($result));
})();

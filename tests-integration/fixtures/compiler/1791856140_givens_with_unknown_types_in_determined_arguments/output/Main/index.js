export function needsGlobal(globalIntBooleanDict) {
  return 0 | 0;
}
export function needsLocal(localIntBooleanDict) {
  return 0 | 0;
}
export function usesLocal(localIntInvalidDict) {
  return /* @__PURE__ */ needsLocal(localIntInvalidDict);
}
export const globalIntInvalid = {};
export const usesGlobal = /* @__PURE__ */ needsGlobal(globalIntInvalid);

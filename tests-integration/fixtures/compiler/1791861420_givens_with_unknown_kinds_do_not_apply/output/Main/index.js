export function needsGlobal(globalIntDict) {
  return 0 | 0;
}
export function needsLocal(localIntDict) {
  return 0 | 0;
}
export function usesLocal(localIntDict) {
  return /* @__PURE__ */ needsLocal(localIntDict);
}
export const globalInt = {};
export const usesGlobal = /* @__PURE__ */ needsGlobal(globalInt);

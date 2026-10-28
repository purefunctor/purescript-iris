export function doubled(value) {
  let ref = value;
  return ref = ref * (2 | 0) | 0;
}
export function offset(value) {
  const $left = 1 | 0;
  let $result;
  $result = value;
  return $left + $result | 0;
}
export function performedAction(action) {
  return action();
}
export function runAction(action) {
  return action();
}
export const performed = /* @__PURE__ */ (() => {
  let ref = 20 | 0;
  return ref = ref + (1 | 0) | 0;
})();

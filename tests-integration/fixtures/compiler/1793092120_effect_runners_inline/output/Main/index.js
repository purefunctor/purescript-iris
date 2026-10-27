export function doubled(value) {
  const ref = { value };
  return ref.value = ref.value * (2 | 0) | 0;
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
  const ref = { value: 20 | 0 };
  return ref.value = ref.value + (1 | 0) | 0;
})();

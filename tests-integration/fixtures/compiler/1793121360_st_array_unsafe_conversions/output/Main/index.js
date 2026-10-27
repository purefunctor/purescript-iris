export function shared(values) {
  const array = values;
  array.push(0 | 0);
  return array;
}
export const literal = /* @__PURE__ */ (() => {
  const $value = [1 | 0, 2 | 0];
  const array = $value;
  array.push(3 | 0);
  return array;
})();

export const freshEffect = /* @__PURE__ */ (() => {
  const $value = () => {
    return 1 | 0;
  };
  return () => {
    return { value: $value };
  };
})();
export const freshFunction = /* @__PURE__ */ (() => {
  const $value = (x) => x + (1 | 0) | 0;
  return () => {
    return { value: $value };
  };
})();

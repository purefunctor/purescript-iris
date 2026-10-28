export function bump(ref) {
  return () => {
    ref.value = ref.value + (1 | 0) | 0;
  };
}
export const passed = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 0 | 0 };
    const $action = bump(ref);
    const $unit = $action();
    ref.value = ref.value + (10 | 0) | 0;
    return ref.value;
  };
})();
export const returned = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 1 | 0 };
    ref.value = 2 | 0;
    return ref;
  };
})();
export const stored = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 3 | 0 };
    ref.value = ref.value * (2 | 0) | 0;
    const $value = { counter: ref };
    return $value;
  };
})();
export const escapesFromClosure = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 0 | 0 };
    const $effect = () => {
      ref.value = ref.value + (1 | 0) | 0;
    };
    const increment = $effect;
    const $unit = increment();
    const $value = ($unit$1) => ref;
    return $value;
  };
})();
export const passedLater = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 0 | 0 };
    const $action = bump(ref);
    const $effect = () => {
      const $unit = $action();
      return ref.value;
    };
    return $effect;
  };
})();

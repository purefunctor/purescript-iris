export function branches(flag) {
  return () => {
    const ref = { value: "start" };
    if (flag) {
      ref.value = "then";
      const $function = (section111) => section111 + "!";
      return $function(ref.value);
    } else {
      ref.value = ref.value + "-else";
      return ref.value;
    }
  };
}
export function stateWrites(start) {
  const ref = { value: start };
  const $value = start * (2 | 0) | 0;
  const written = ref.value = $value;
  const modified = ref.value = ref.value + (1 | 0) | 0;
  const final = ref.value;
  const $value$1 = [
    written,
    modified,
    final
  ];
  return $value$1;
}
export const refs = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 0 | 0 };
    ref.value = 1 | 0;
    ref.value = ref.value + (1 | 0) | 0;
    return ref.value;
  };
})();

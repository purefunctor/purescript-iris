export function branches(flag) {
  return () => {
    let ref = "start";
    if (flag) {
      ref = "then";
      const $function = (section111) => section111 + "!";
      return $function(ref);
    } else {
      ref = ref + "-else";
      return ref;
    }
  };
}
export function stateWrites(start) {
  let ref = start;
  const $value = start * (2 | 0) | 0;
  const written = ref = $value;
  const modified = ref = ref + (1 | 0) | 0;
  const final = ref;
  const $value$1 = [
    written,
    modified,
    final
  ];
  return $value$1;
}
export const refs = /* @__PURE__ */ (() => {
  return () => {
    let ref = 0 | 0;
    ref = 1 | 0;
    ref = ref + (1 | 0) | 0;
    return ref;
  };
})();

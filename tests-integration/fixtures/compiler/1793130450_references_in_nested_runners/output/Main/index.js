export function runInside(start) {
  let ref = start;
  let $result;
  const local = { value: start };
  const $effect = () => {
    return local.value = local.value + (1 | 0) | 0;
  };
  const bump = $effect;
  const $int = bump();
  $result = bump();
  const inner = $result;
  ref = ref + inner | 0;
  return ref;
}
export const performedInside = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 1 | 0 };
    let $result;
    ref.value = ref.value * (2 | 0) | 0;
    $result = ref.value;
    const doubled = $result;
    ref.value = ref.value + doubled | 0;
    return ref.value;
  };
})();

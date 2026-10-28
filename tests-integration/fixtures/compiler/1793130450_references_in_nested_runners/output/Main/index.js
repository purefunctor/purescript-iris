export function runInside(start) {
  let ref = start;
  let $result;
  let local = start;
  const $effect = () => {
    return local = local + (1 | 0) | 0;
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
    let ref = 1 | 0;
    let $result;
    ref = ref * (2 | 0) | 0;
    $result = ref;
    const doubled = $result;
    ref = ref + doubled | 0;
    return ref;
  };
})();

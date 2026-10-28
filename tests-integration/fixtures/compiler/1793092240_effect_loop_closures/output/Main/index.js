export function elementClosures(values) {
  const $value = [];
  return () => {
    let closures = $value;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      closures = [...closures, ($unit) => value];
    }
    return closures;
  };
}
export const rangeClosures = /* @__PURE__ */ (() => {
  const $value = [];
  return () => {
    let closures = $value;
    for (let index = 0 | 0; index < (3 | 0); index++) {
      closures = [...closures, ($unit) => index];
    }
    return closures;
  };
})();
export const stateClosures = /* @__PURE__ */ (() => {
  const $value = [];
  let closures = $value;
  for (let index = 5 | 0; index < (8 | 0); index++) {
    closures = [...closures, ($unit) => index];
  }
  return closures;
})();

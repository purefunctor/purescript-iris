export function counted(values) {
  return () => {
    let total = 0 | 0;
    for (let index = 0 | 0; index < (3 | 0); index++) {
      total = total + index | 0;
    }
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      total = total + value | 0;
    }
    const $function = (section107) => section107 < (20 | 0);
    while (true) {
      let $condition;
      $condition = $function(total);
      if (!$condition) {
        break;
      }
      total = total + (4 | 0) | 0;
    }
    while (true) {
      let $condition$1;
      total = total + (1 | 0) | 0;
      const $function$1 = (section157) => section157 >= (25 | 0);
      $condition$1 = $function$1(total);
      if ($condition$1) {
        break;
      }
    }
    return total;
  };
}
export function stateCountdown(start) {
  let remaining = start;
  const $value = [];
  let seen = $value;
  const $function = (section291) => section291 > (0 | 0);
  while (true) {
    let $condition;
    $condition = $function(remaining);
    if (!$condition) {
      break;
    }
    const value = remaining;
    seen = [...seen, value];
    remaining = remaining - (1 | 0) | 0;
  }
  return seen;
}
export const doubledPerIteration = /* @__PURE__ */ (() => {
  return () => {
    let total = 0 | 0;
    for (let index = 1 | 0; index < (4 | 0); index++) {
      let local = index;
      local = local * (2 | 0) | 0;
      const value = local;
      total = total + value | 0;
    }
    return total;
  };
})();

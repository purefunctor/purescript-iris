export function counted(values) {
  return () => {
    const total = { value: 0 | 0 };
    for (let index = 0 | 0; index < (3 | 0); index++) {
      total.value = total.value + index | 0;
    }
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      total.value = total.value + value | 0;
    }
    const $function = (section107) => section107 < (20 | 0);
    while (true) {
      let $condition;
      $condition = $function(total.value);
      if (!$condition) {
        break;
      }
      total.value = total.value + (4 | 0) | 0;
    }
    while (true) {
      let $condition$1;
      total.value = total.value + (1 | 0) | 0;
      const $function$1 = (section157) => section157 >= (25 | 0);
      $condition$1 = $function$1(total.value);
      if ($condition$1) {
        break;
      }
    }
    return total.value;
  };
}
export function stateCountdown(start) {
  const remaining = { value: start };
  const $value = [];
  const seen = { value: $value };
  const $function = (section291) => section291 > (0 | 0);
  while (true) {
    let $condition;
    $condition = $function(remaining.value);
    if (!$condition) {
      break;
    }
    const value = remaining.value;
    seen.value = [...seen.value, value];
    remaining.value = remaining.value - (1 | 0) | 0;
  }
  return seen.value;
}
export const doubledPerIteration = /* @__PURE__ */ (() => {
  return () => {
    const total = { value: 0 | 0 };
    for (let index = 1 | 0; index < (4 | 0); index++) {
      const local = { value: index };
      local.value = local.value * (2 | 0) | 0;
      const value = local.value;
      total.value = total.value + value | 0;
    }
    return total.value;
  };
})();

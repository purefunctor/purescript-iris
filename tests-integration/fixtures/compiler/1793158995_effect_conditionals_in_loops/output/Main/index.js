import * as Data_Unit from "../Data.Unit/index.js";
export function loopBody(limit) {
  return () => {
    let total = 0 | 0;
    for (let index = 0 | 0; index < limit; index++) {
      if (index < (3 | 0)) {
        total = total + index | 0;
      }
      total = total + (100 | 0) | 0;
    }
    return total;
  };
}
export function nestedConditionals(count) {
  return () => {
    let ref = 0 | 0;
    let label;
    if (count > (0 | 0)) {
      if (count > (10 | 0)) {
        ref = 2 | 0;
        label = 20 | 0;
      } else {
        ref = 1 | 0;
        label = 10 | 0;
      }
    } else {
      label = 0 | 0;
    }
    ref = ref + label | 0;
    return ref;
  };
}
export function tailConditional(count) {
  let ref = count;
  if (count > (0 | 0)) {
    return ref = ref * (2 | 0) | 0;
  } else {
    return ref;
  }
}

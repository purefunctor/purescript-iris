import * as Data_Unit from "../Data.Unit/index.js";
export function loopBody(limit) {
  return () => {
    let total = 0 | 0;
    for (let index = 0 | 0; index < limit; index++) {
      let $result;
      if (index < (3 | 0)) {
        $result = () => {
          total = total + index | 0;
        };
      } else {
        const $value = Data_Unit.unit;
        $result = () => {
          return $value;
        };
      }
      const $unit = $result();
      total = total + (100 | 0) | 0;
    }
    return total;
  };
}
export function nestedConditionals(count) {
  return () => {
    let ref = 0 | 0;
    let $result;
    if (count > (0 | 0)) {
      if (count > (10 | 0)) {
        $result = () => {
          ref = 2 | 0;
          return 20 | 0;
        };
      } else {
        $result = () => {
          ref = 1 | 0;
          return 10 | 0;
        };
      }
    } else {
      $result = () => {
        return 0 | 0;
      };
    }
    const label = $result();
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

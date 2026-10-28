export function statementIf(flag) {
  return () => {
    let ref = 0 | 0;
    let $result;
    if (flag) {
      $result = () => {
        ref = 1 | 0;
      };
    } else {
      $result = () => {
        ref = 2 | 0;
      };
    }
    const $unit = $result();
    ref = ref + (10 | 0) | 0;
    return ref;
  };
}
export function boundIf(flag) {
  return () => {
    let ref = 5 | 0;
    let $result;
    if (flag) {
      $result = () => {
        return ref;
      };
    } else {
      $result = () => {
        return 0 | 0;
      };
    }
    const chosen = $result();
    ref = 7 | 0;
    const $value = chosen + (1 | 0) | 0;
    return $value;
  };
}
export function opaqueBranches(flag) {
  return (perform) => {
    return (produce) => {
      return () => {
        let ref = 0 | 0;
        let $result;
        if (flag) {
          $result = perform;
        } else {
          $result = () => {
            ref = 1 | 0;
          };
        }
        const $unit = $result();
        let $result$1;
        if (flag) {
          $result$1 = produce;
        } else {
          $result$1 = () => {
            return ref;
          };
        }
        const result = $result$1();
        const $value = result + (1 | 0) | 0;
        return $value;
      };
    };
  };
}

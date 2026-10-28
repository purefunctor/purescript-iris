export function statementIf(flag) {
  return () => {
    let ref = 0 | 0;
    if (flag) {
      ref = 1 | 0;
    } else {
      ref = 2 | 0;
    }
    ref = ref + (10 | 0) | 0;
    return ref;
  };
}
export function boundIf(flag) {
  return () => {
    let ref = 5 | 0;
    let chosen;
    if (flag) {
      chosen = ref;
    } else {
      chosen = 0 | 0;
    }
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
        if (flag) {
          perform();
        } else {
          ref = 1 | 0;
        }
        let result;
        if (flag) {
          result = produce();
        } else {
          result = ref;
        }
        const $value = result + (1 | 0) | 0;
        return $value;
      };
    };
  };
}

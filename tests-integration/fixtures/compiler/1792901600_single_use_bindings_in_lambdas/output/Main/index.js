export function sharedIntegerAdd(left) {
  return (right) => {
    return (value) => (left + right | 0) + value | 0;
  };
}
export function sharedRecord(value) {
  return ($unit) => ({ value });
}
export function directUse(left) {
  return (right) => {
    return (left + right | 0) * (2 | 0) | 0;
  };
}
export function branchUse(flag) {
  return (left) => {
    return (right) => {
      if (flag) {
        return left + right | 0;
      } else {
        return 0 | 0;
      }
    };
  };
}

export function sharedIntegerAdd(left) {
  return (right) => {
    const total = left + right | 0;
    return (value) => total + value | 0;
  };
}
export function sharedRecord(value) {
  const record = { value };
  return ($unit) => record;
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

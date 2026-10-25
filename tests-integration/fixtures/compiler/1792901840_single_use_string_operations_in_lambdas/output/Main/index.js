export function sharedAppend(value) {
  const doubled = value + value;
  return ($unit) => doubled;
}
export function sharedEqual(left) {
  return (right) => {
    const matches = left === right;
    return (value) => matches ? value : 0 | 0;
  };
}

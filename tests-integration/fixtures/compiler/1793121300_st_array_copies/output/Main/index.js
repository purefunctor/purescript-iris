export function copies(original) {
  const array = original.slice();
  array.push(3 | 0);
  const frozen = array.slice();
  const clone = array.slice();
  array.push(4 | 0);
  clone.push(5 | 0);
  const cloned = clone.slice();
  const final = array.slice();
  const $value = {
    original,
    frozen,
    cloned,
    final
  };
  return $value;
}

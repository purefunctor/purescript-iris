export function built(key) {
  const object = {};
  object.a = 1 | 0;
  object[key] = 2 | 0;
  const poked = object;
  poked.removed = 3 | 0;
  delete poked.removed;
  return poked;
}
export function copies(original) {
  const object = { ...original };
  object.b = 2 | 0;
  const frozen = { ...object };
  delete object.a;
  const final = { ...object };
  const $value = {
    original,
    frozen,
    final
  };
  return $value;
}

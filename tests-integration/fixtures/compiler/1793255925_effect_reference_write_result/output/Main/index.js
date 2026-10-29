export function writeResult(reference) {
  return () => {
    return reference.value = 7 | 0;
  };
}
export function boundWriteResult(reference) {
  return () => {
    const result = reference.value = 8 | 0;
    return result;
  };
}
export function localWriteResult(value) {
  return () => {
    let reference = value;
    const $value = value + (1 | 0) | 0;
    const result = reference = $value;
    return result;
  };
}

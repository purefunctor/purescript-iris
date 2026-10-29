export function writeResult(reference) {
  return () => {
    reference.value = 7 | 0;
    return undefined;
  };
}
export function boundWriteResult(reference) {
  return () => {
    reference.value = 8 | 0;
    return undefined;
  };
}
export function localWriteResult(value) {
  return () => {
    let reference = value;
    const $value = value + (1 | 0) | 0;
    reference = $value;
    return undefined;
  };
}

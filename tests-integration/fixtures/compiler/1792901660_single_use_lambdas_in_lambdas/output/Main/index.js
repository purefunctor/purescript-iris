export function sharedFunction(offset) {
  return (value) => ((value$1) => value$1 + offset | 0)(value);
}

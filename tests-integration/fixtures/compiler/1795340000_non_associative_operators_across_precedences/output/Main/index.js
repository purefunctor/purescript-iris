export function equal(left) {
  return (right) => {
    return left;
  };
}
export function tilde(left) {
  return (right) => {
    return left;
  };
}
export const lowerThenHigher = equal(1 | 0)(tilde(2 | 0)(3 | 0));
export const higherThenLower = equal(tilde(1 | 0)(2 | 0))(3 | 0);

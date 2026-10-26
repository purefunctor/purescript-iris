export function topLevel(x) {
  return (x$1) => {
    return x$1;
  };
}
export function whereBound(a) {
  return ((y) => (y$1) => y$1)(a)(a);
}
export function combine(dictionary) {
  return dictionary.combine;
}
export const combineInt = { combine: (z) => (z$1) => z$1 };

export const sameReference = (left) => (right) => left === right;

export const incrementForeign = (reference) => () => {
  reference.value += 1;
};

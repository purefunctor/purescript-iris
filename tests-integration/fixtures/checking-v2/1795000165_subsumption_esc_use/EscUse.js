export const newRef = (a) => () => ({ value: a });
export const run = (st) => st();
export const readRef = (ref) => () => ref.value;

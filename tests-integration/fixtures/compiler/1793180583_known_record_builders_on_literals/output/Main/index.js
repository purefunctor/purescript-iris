export function inserted(value) {
  return {
    x: 1 | 0,
    y: 2 | 0,
    z: value
  };
}
export function reshaped(value) {
  return {
    x: (1 | 0) + value | 0,
    w: 3 | 0,
    z: "new"
  };
}
export function fromScratch(value) {
  return {
    x: value,
    y: 2 | 0
  };
}
export const composedBackwards = {
  x: 1 | 0,
  y: (4 | 0) * (10 | 0) | 0
};
export const merged = {
  x: 1 | 0,
  y: 20 | 0,
  z: 30 | 0
};

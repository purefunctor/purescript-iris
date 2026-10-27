export function binding(name, initialize) {
  let state = 0;
  let value;
  return () => {
    if (state === 2) return value;
    if (state === 1) {
      throw new ReferenceError(`${name} was needed before it finished initializing`);
    }
    state = 1;
    value = initialize();
    state = 2;
    return value;
  };
}

export function intDiv(x, y) {
  if (y === 0) return 0;
  return y > 0 ? Math.floor(x / y) : -Math.floor(x / -y);
}

export function intMod(x, y) {
  if (y === 0) return 0;
  const yy = Math.abs(y);
  return ((x % yy) + yy) % yy;
}

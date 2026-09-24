export function apply(f) {
  return (x) => {
    return f(x);
  };
}
export function identity(a) {
  return a;
}
export function dimap(dictionary) {
  return dictionary.dimap;
}
export function lens(pack) {
  return (unpack) => {
    return (profunctorPDict) => (pab) => /* @__PURE__ */ dimap(profunctorPDict)(unpack)(pack)(pab);
  };
}
export function wrapped(profunctorPDict) {
  const $closure = (section183) => {
    const a = section183;
    return a;
  };
  return apply(lens((value) => value))($closure);
}
export const profunctorFn = { dimap: (ab) => (cd) => (bc) => (a) => cd(bc(ab(a))) };

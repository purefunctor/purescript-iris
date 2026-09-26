import * as $foreign from "./foreign.js";
export function variable(section16) {
  if (Array.isArray(section16) && section16.length === 1) {
    const x = section16[0];
    return x;
  }
  {
    const xs = section16;
    const $scrutinee = arrayLength(xs);
    const length = $scrutinee;
    return length;
  }
  throw new Error("Pattern match failure");
}
export function wildcard(section49) {
  if (Array.isArray(section49) && section49.length === 1) {
    const x = section49[0];
    return x;
  }
  {
    const xs = section49;
    const $scrutinee = arrayLength(xs);
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function refutable(section82) {
  const $closure = (partialDict) => {
    if (Array.isArray(section82) && section82.length === 1) {
      const x = section82[0];
      return x;
    }
    {
      const xs = section82;
      if (Array.isArray(xs) && xs.length === 1) {
        const y = xs[0];
        return y;
      }
    }
    throw new Error("Pattern match failure");
  };
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $closure($result);
}
export const arrayLength = $foreign["arrayLength"];

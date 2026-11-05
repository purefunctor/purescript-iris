export function singleton(dictionary) {
  return dictionary.singleton;
}
export function produce(dictionary) {
  return dictionary.produce;
}
export function identity(x) {
  return x;
}
export function test(buildFDict) {
  return (section62) => {
    return (section63) => {
      {
        const from = section62;
        const to = section63;
        if (from) {
          return /* @__PURE__ */ singleton(buildFDict)(true);
        }
        if (to) {
          return /* @__PURE__ */ produce(buildFDict)(identity)(true);
        }
        if (true) {
          return /* @__PURE__ */ produce(buildFDict)(identity)(false);
        }
      }
      throw new Error("Pattern match failure");
    };
  };
}

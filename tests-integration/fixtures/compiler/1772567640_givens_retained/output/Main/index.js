export function consume(dictionary) {
  return dictionary.consume;
}
export function testGiven(givenADict) {
  return (a) => {
    const b = /* @__PURE__ */ consume(givenADict)(a);
    const c = /* @__PURE__ */ consume(givenADict)(a);
    return /* @__PURE__ */ consume(givenADict)(a);
  };
}

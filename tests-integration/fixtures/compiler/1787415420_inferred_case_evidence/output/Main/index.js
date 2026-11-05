export function empty(dictionary) {
  return dictionary.empty;
}
export function chooseEmpty(emptyCollectionDict) {
  return (section33) => {
    if (section33 === true) {
      return /* @__PURE__ */ empty(emptyCollectionDict);
    }
    if (section33 === false) {
      return /* @__PURE__ */ empty(emptyCollectionDict);
    }
    throw new Error("Pattern match failure");
  };
}

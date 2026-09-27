export function invalidAdd(addDict) {
  return "Proxy";
}
export function invalidCompare(compareLTDict) {
  return "Proxy";
}
export function invalidAppend(appendDict) {
  return "Proxy";
}
export function invalidCons(consDict) {
  return "Proxy";
}
export const forceSolve = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  const $field = /* @__PURE__ */ invalidAdd($result);
  let $result$1;
  throw new Error("Generated code reached a source error");
  const $field$1 = /* @__PURE__ */ invalidCompare($result$1);
  let $result$2;
  throw new Error("Generated code reached a source error");
  const $field$2 = /* @__PURE__ */ invalidAppend($result$2);
  let $result$3;
  throw new Error("Generated code reached a source error");
  return {
    invalidAdd: $field,
    invalidCompare: $field$1,
    invalidAppend: $field$2,
    invalidCons: /* @__PURE__ */ invalidCons($result$3)
  };
})();
export const forceSolveChecked = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  const $field = /* @__PURE__ */ invalidCompare($result);
  let $result$1;
  throw new Error("Generated code reached a source error");
  return {
    invalidCompare: $field,
    invalidCons: /* @__PURE__ */ invalidCons($result$1)
  };
})();

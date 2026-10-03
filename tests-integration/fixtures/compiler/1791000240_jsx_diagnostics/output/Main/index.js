import { jsx as $jsx, jsxs as $jsxs, Fragment as $Fragment } from "react/jsx-runtime";
export const button = /* @__PURE__ */ (() => {
  return ($record) => "button";
})();
export const test = /* @__PURE__ */ (() => {
  return $jsx(button, { count: "not an Int" });
})();
export const test2 = /* @__PURE__ */ (() => {
  return $jsx(button, {});
})();
export const test3 = /* @__PURE__ */ (() => {
  return $jsx("div", {
    title: "first",
    title: "second"
  });
})();
export const test4 = /* @__PURE__ */ (() => {
  return $jsx("div", { children: $jsx("span", {}) });
})();
export const test5 = (() => {
  throw new Error("Generated code reached a source error");
})();
export const test6 = /* @__PURE__ */ (() => {
  return $jsx("div", { children: 42 | 0 });
})();
export const test7 = /* @__PURE__ */ (() => {
  return $jsxs($Fragment, { children: [$jsx("div", {})] });
})();
export const notCallable = 42 | 0;
export const test8 = /* @__PURE__ */ (() => {
  return $jsx(notCallable, {});
})();
export const test9 = /* @__PURE__ */ (() => {
  return $jsx(button, {
    count: 1 | 0,
    unknown: "extra"
  });
})();
export const test10 = /* @__PURE__ */ (() => {
  const $reactKey = 42 | 0;
  return $jsx(button, { count: 1 | 0 }, $reactKey);
})();
export const test11 = /* @__PURE__ */ (() => {
  return $jsx("div", {
    children: null,
    children: "duplicate"
  });
})();
export const child = /* @__PURE__ */ (() => {
  return (props) => props.children;
})();
export const test12 = /* @__PURE__ */ (() => {
  return $jsx(child, {});
})();

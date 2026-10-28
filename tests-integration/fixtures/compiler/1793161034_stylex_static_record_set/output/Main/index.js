import * as $stylex from "@stylexjs/stylex";
export const base = {
  color: "red",
  padding: 4 | 0
};
export const styles = /* @__PURE__ */ (() => {
  return $stylex.create({
    primary: {
      ...base,
      color: "blue"
    },
    spaced: {
      ...base,
      margin: 2 | 0
    }
  });
})();

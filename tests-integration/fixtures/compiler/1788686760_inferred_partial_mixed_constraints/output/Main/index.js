import * as $foreign from "./foreign.js";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function select(dictionary) {
  return dictionary.select;
}
export function mixed(partialDict) {
  return (selectDict) => {
    return (choice) => {
      return (fallback) => {
        if (choice.tag === "Just") {
          const { _1: value } = choice;
          return /* @__PURE__ */ select(selectDict)(value)(fallback);
        } else {
          throw new Error("Pattern match failure");
        }
      };
    };
  };
}
export const unsafePartial = $foreign["unsafePartial"];
export const selectInt = { select: (first) => (second) => second };
export const discharged = unsafePartial((partialDict) => /* @__PURE__ */ mixed(partialDict)(selectInt));

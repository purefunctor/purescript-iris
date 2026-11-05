import * as $foreign from "./foreign.js";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function inferred(partialDict) {
  return (choice) => {
    if (choice.tag === "Just") {
      const { _1: value } = choice;
      return value;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function signed(partialDict) {
  return (choice) => /* @__PURE__ */ inferred(partialDict)(choice);
}
export const unsafePartial = $foreign["unsafePartial"];
export const discharged = unsafePartial((partialDict) => /* @__PURE__ */ inferred(partialDict));

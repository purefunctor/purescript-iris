import * as $foreign from "./foreign.js";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function unwrap(dictionary) {
  return dictionary.unwrap;
}
export const unsafePartial = $foreign["unsafePartial"];
export const unwrapMaybe = /* @__PURE__ */ (() => {
  return { unwrap: (partialDict) => {
    return (choice) => {
      if (choice.tag === "Just") {
        const { _1: value } = choice;
        return value;
      } else {
        throw new Error("Pattern match failure");
      }
    };
  } };
})();
export const discharged = unsafePartial((partialDict) => /* @__PURE__ */ unwrap(unwrapMaybe)(partialDict));

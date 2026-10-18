import * as $foreign from "./foreign.js";
export function render(state) {
  return node("main")([attribute("root")])([node("span")([attribute(((value) => value ? "active" : "inactive")(state))])([text("first")]), node("span")([])([text("second")])]);
}
export const node = $foreign["node"];
export const attribute = $foreign["attribute"];
export const text = $foreign["text"];

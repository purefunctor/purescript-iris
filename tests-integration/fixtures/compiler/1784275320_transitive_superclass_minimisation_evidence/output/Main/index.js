import * as $foreign from "./foreign.js";
export function test(topDict) {
  return (value) => {
    const Middle0Dict = /* @__PURE__ */ topDict.Middle0();
    const $scrutinee = /* @__PURE__ */ useTop(topDict)(value);
    const $scrutinee$1 = /* @__PURE__ */ useMiddle(Middle0Dict)(value);
    return /* @__PURE__ */ useBottom(/* @__PURE__ */ Middle0Dict.Bottom0())(value);
  };
}
export const useBottom = $foreign["useBottom"];
export const useMiddle = $foreign["useMiddle"];
export const useTop = $foreign["useTop"];

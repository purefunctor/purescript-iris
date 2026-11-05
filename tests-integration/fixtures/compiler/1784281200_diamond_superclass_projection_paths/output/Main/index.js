import * as $foreign from "./foreign.js";
export function testDiamond(topDict) {
  return (value) => {
    const Left0Dict = /* @__PURE__ */ topDict.Left0();
    const $scrutinee = /* @__PURE__ */ useTop(topDict)(value);
    const $scrutinee$1 = /* @__PURE__ */ useLeft(Left0Dict)(value);
    const $scrutinee$2 = /* @__PURE__ */ useRight(/* @__PURE__ */ topDict.Right1())(value);
    return /* @__PURE__ */ useBottom(/* @__PURE__ */ Left0Dict.Bottom0())(value);
  };
}
export function testShared(leftDict) {
  return (rightDict) => {
    return (value) => {
      const $scrutinee = /* @__PURE__ */ useLeft(leftDict)(value);
      const $scrutinee$1 = /* @__PURE__ */ useRight(rightDict)(value);
      return /* @__PURE__ */ useBottom(/* @__PURE__ */ leftDict.Bottom0())(value);
    };
  };
}
export const useBottom = $foreign["useBottom"];
export const useLeft = $foreign["useLeft"];
export const useRight = $foreign["useRight"];
export const useTop = $foreign["useTop"];

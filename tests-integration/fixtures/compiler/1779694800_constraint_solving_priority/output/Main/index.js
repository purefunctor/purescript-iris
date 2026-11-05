import * as $foreign from "./foreign.js";
export const M = ($value0) => ({
  tag: "M",
  _1: $value0
});
export function bind($m) {
  if ($m.tag === "M") {
    const { _1: a } = $m;
    return (f) => {
      return f(a);
    };
  } else {
    throw new Error("Pattern match failure");
  }
}
export function ask(dictionary) {
  return dictionary.ask;
}
export function consume(rowToListRowListDict) {
  return ($record) => ({});
}
export function produce(dictionary) {
  return dictionary.produce;
}
export function produce1(rowToListInputInputListDict) {
  return { produce: ($record) => (input) => input };
}
export function test(rowToListOutputOutputListDict) {
  return (produceContextInputOutputDict) => {
    return (input) => {
      return bind(/* @__PURE__ */ ask(askM))((context) => {
        const output = /* @__PURE__ */ produce(produceContextInputOutputDict)(context)(input);
        return {
          tag: "M",
          _1: /* @__PURE__ */ consume(rowToListOutputOutputListDict)(output)
        };
      });
    };
  };
}
export const unsafeCoerce = $foreign["unsafeCoerce"];
export const askM = { ask: unsafeCoerce({
  tag: "M",
  _1: {}
}) };

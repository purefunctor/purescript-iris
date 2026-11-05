import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function forCaptured(counter) {
  return Effect.forE(0 | 0)(mark("high")(3 | 0))(mark("callback")((index) => {
    return () => {
      const section90 = counter.value;
      const $value = section90 + index | 0;
      counter.value = $value;
      return undefined;
    };
  }));
}
export function forInlineBody(counter) {
  return Effect.forE(0 | 0)(3 | 0)((index) => {
    const $value = mark("body")(index);
    return () => {
      counter.value = $value;
      return undefined;
    };
  });
}
export function untilCaptured(counter) {
  const $function = Effect.untilE;
  const $function$1 = mark("action");
  const $function$2 = (section160) => section160 >= (2 | 0);
  const $effect = () => {
    let $value;
    const section172 = counter.value;
    const $value$1 = section172 + (1 | 0) | 0;
    $value = counter.value = $value$1;
    return $function$2($value);
  };
  return $function($function$1($effect));
}
export function whileCaptured(counter) {
  const $function = Effect.whileE;
  const $function$1 = mark("condition");
  const $function$2 = (section206) => section206 < (3 | 0);
  const $effect = () => {
    return $function$2(counter.value);
  };
  const $function$3 = $function($function$1($effect));
  const $function$4 = mark("action");
  const $effect$1 = () => {
    const section231 = counter.value;
    const $value = section231 + (1 | 0) | 0;
    counter.value = $value;
    return undefined;
  };
  return $function$3($function$4($effect$1));
}
export function foreachCaptured(counter) {
  return (elements) => {
    return Effect.foreachE(mark("elements")(elements))(mark("callback")((element) => {
      return () => {
        const section281 = counter.value;
        const $value = section281 + element | 0;
        counter.value = $value;
        return undefined;
      };
    }));
  };
}
export function forPartial(count) {
  return Effect.forE(0 | 0)(count);
}
export const mark = $foreign["mark"];
export const fresh = /* @__PURE__ */ (() => {
  return () => {
    return { value: 0 | 0 };
  };
})();

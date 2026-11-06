import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function forCaptured(counter) {
  const $end = mark("high")(3 | 0);
  const $body = mark("callback")((index) => {
    return () => {
      const section90 = counter.value;
      const $value = section90 + index | 0;
      counter.value = $value;
      return undefined;
    };
  });
  return () => {
    for (let $index = 0 | 0; $index < $end; $index++) {
      $body($index)();
    }
    return;
  };
}
export function forInlineBody(counter) {
  return () => {
    for (let index = 0 | 0; index < (3 | 0); index++) {
      const $value = mark("body")(index);
      counter.value = $value;
    }
    return;
  };
}
export function untilCaptured(counter) {
  const $function = mark("action");
  const $function$1 = (section160) => section160 >= (2 | 0);
  const $condition = $function(() => {
    let $value;
    const section172 = counter.value;
    const $value$1 = section172 + (1 | 0) | 0;
    $value = counter.value = $value$1;
    return $function$1($value);
  });
  return () => {
    while (!$condition()) {}
    return;
  };
}
export function whileCaptured(counter) {
  const $function = mark("condition");
  const $function$1 = (section206) => section206 < (3 | 0);
  const $condition = $function(() => {
    return $function$1(counter.value);
  });
  const $function$2 = mark("action");
  const $body = $function$2(() => {
    const section231 = counter.value;
    const $value = section231 + (1 | 0) | 0;
    counter.value = $value;
    return undefined;
  });
  return () => {
    while ($condition()) {
      $body();
    }
    return;
  };
}
export function foreachCaptured(counter) {
  return (elements) => {
    const $array = mark("elements")(elements);
    const $body = mark("callback")((element) => {
      return () => {
        const section281 = counter.value;
        const $value = section281 + element | 0;
        counter.value = $value;
        return undefined;
      };
    });
    return () => {
      const $length = $array.length;
      for (let $index = 0; $index < $length; $index++) {
        $body($array[$index])();
      }
      return;
    };
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

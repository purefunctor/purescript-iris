import * as $foreign from "./foreign.js";
import * as $runtime from "../runtime.js";
export function applicationRecursive(token) {
  let $lazy_first;
  let $lazy_second;
  $lazy_first = $runtime.binding("first", () => {
    return observe("first")((condition) => condition ? $lazy_second()(false) : 1 | 0);
  });
  $lazy_second = $runtime.binding("second", () => {
    return observe("second")((condition$1) => condition$1 ? $lazy_first()(false) : 2 | 0);
  });
  const first = $lazy_first();
  const second = $lazy_second();
  return {
    result: first(true),
    trace: readTrace(token)
  };
}
export function caseRecursive(condition) {
  let $lazy_go;
  $lazy_go = $runtime.binding("go", () => {
    if (condition === true) {
      return (current) => current ? $lazy_go()(false) : 30 | 0;
    }
    if (condition === false) {
      return ($boolean) => 31 | 0;
    }
    throw new Error("Pattern match failure");
  });
  const go = $lazy_go();
  return go(true);
}
export function letRecursive(condition) {
  const go = (current) => {
    let $argument0 = current;
    while (true) {
      const $currentArgument0 = $argument0;
      if ($currentArgument0) {
        $argument0 = false;
        continue;
      } else {
        return 40 | 0;
      }
    }
  };
  return go(condition);
}
export function strictCycle($boolean) {
  let $lazy_value;
  $lazy_value = $runtime.binding("value", () => {
    return wrap($lazy_value());
  });
  const value = $lazy_value();
  return value;
}
export function wrap(value) {
  return value;
}
export const same = $foreign["same"];
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const recordRecursive = /* @__PURE__ */ (() => {
  let $lazy_first;
  let $lazy_second;
  $lazy_first = $runtime.binding("first", () => {
    return { value: $lazy_second() };
  });
  $lazy_second = $runtime.binding("second", () => {
    return (condition) => condition ? $lazy_first().value(false) : 20 | 0;
  });
  const first = $lazy_first();
  const second = $lazy_second();
  return same(first.value)(second);
})();

import * as Control_Bind from "../Control.Bind/index.js";
import * as Effect from "../Effect/index.js";
export function doBind(action) {
  const $closure = ($tuple) => {
    if ($tuple.tag === "Tuple") {
      const { _1: x, _2: x$1 } = $tuple;
      return () => {
        return x$1;
      };
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return /* @__PURE__ */ Control_Bind.bind(Effect.bindEffect)(action)($closure);
}
export function adoBind(action) {
  const $closure = ($tuple) => {
    if ($tuple.tag === "Tuple") {
      const { _1: x, _2: x$1 } = $tuple;
      return x$1;
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return () => {
    return $closure(action());
  };
}
export function successiveBinds(action) {
  return () => {
    const x = action();
    const $value = x + (1 | 0) | 0;
    return $value;
  };
}

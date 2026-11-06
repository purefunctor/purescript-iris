import * as Data_Array from "../Data.Array/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Function from "../Data.Function/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect_Exception from "../Effect.Exception/index.js";
export function describe(section75) {
  if (section75.tag === "Left") {
    const { _1: error } = section75;
    return {
      tag: "Left",
      _1: Effect_Exception.message(error)
    };
  }
  if (section75.tag === "Right") {
    const { _1: value } = section75;
    return {
      tag: "Right",
      _1: value
    };
  }
  throw new Error("Pattern match failure");
}
export function observe(body) {
  const $value = [];
  return () => {
    const visited = { value: $value };
    const $action = Effect_Exception.try(body(visited));
    const outcome = $action();
    const seen = visited.value;
    return {
      outcome: describe(outcome),
      visited: seen
    };
  };
}
export function record(visited) {
  return (label) => {
    const modifyFunction = Data_Function.flip(Data_Array.snoc)(label);
    return () => {
      const value = visited.value;
      const $value = modifyFunction(value);
      visited.value = $value;
      return undefined;
    };
  };
}
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
export const forAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      for (let index = 0 | 0; index < (5 | 0); index++) {
        const $condition = index === (3 | 0);
        const $action = Effect_Exception.throw("stop at 3");
        if ($condition) {
          $action();
        }
        record(visited)(/* @__PURE__ */ showIntDictShow(index))();
      }
      return 99 | 0;
    };
  });
})();
export const untilAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      let counter = 0 | 0;
      while (true) {
        let $condition;
        let count;
        const section300 = counter;
        const $value = section300 + (1 | 0) | 0;
        count = counter = $value;
        const $action = record(visited)(/* @__PURE__ */ showIntDictShow(count));
        const $unit = $action();
        const $condition$1 = count === (2 | 0);
        const $action$1 = Effect_Exception.throw("stop at 2");
        if ($condition$1) {
          $action$1();
        }
        $condition = count >= (5 | 0);
        if ($condition) {
          break;
        }
      }
      return counter;
    };
  });
})();
export const whileAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      let counter = 0 | 0;
      const $function = (section384) => section384 < (5 | 0);
      while (true) {
        let $condition;
        $condition = $function(counter);
        if (!$condition) {
          break;
        }
        let count;
        const section406 = counter;
        const $value = section406 + (1 | 0) | 0;
        count = counter = $value;
        const $condition$1 = count === (3 | 0);
        const $action = Effect_Exception.throw("stop at 3");
        if ($condition$1) {
          $action();
        }
        record(visited)(/* @__PURE__ */ showIntDictShow(count))();
      }
      return;
    };
  });
})();
export const foreachAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    const $array = [
      "a",
      "b",
      "c",
      "d"
    ];
    return () => {
      const $length = $array.length;
      for (let $index = 0; $index < $length; $index++) {
        const element = $array[$index];
        const $condition = element === "c";
        const $action = Effect_Exception.throw("stop at c");
        if ($condition) {
          $action();
        }
        record(visited)(element)();
      }
      return;
    };
  });
})();
export const nestedAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      for (let row = 0 | 0; row < (3 | 0); row++) {
        const $action = record(visited)("row " + /* @__PURE__ */ showIntDictShow(row));
        const $unit = $action();
        for (let column = 0 | 0; column < (3 | 0); column++) {
          const $condition = row === (1 | 0) && column === (1 | 0);
          const $action$1 = Effect_Exception.throw("stop at 1:1");
          if ($condition) {
            $action$1();
          }
          record(visited)(/* @__PURE__ */ showIntDictShow(row) + (":" + /* @__PURE__ */ showIntDictShow(column)))();
        }
      }
      return;
    };
  });
})();
export const recovers = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      for (let index = 0 | 0; index < (4 | 0); index++) {
        const $function = Effect_Exception.catchException((error) => record(visited)("caught " + Effect_Exception.message(error)));
        const $condition = index === (1 | 0);
        const $action = Effect_Exception.throw("one");
        $function(() => {
          if ($condition) {
            $action();
          }
          return record(visited)(/* @__PURE__ */ showIntDictShow(index))();
        })();
      }
      return;
    };
  });
})();

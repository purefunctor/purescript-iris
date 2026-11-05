import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Array from "../Data.Array/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Function from "../Data.Function/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
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
    const $action$1 = Effect.forE(0 | 0)(5 | 0)((index) => {
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(index === (3 | 0))(Effect_Exception.throw("stop at 3"));
      return () => {
        const $unit = $action();
        return record(visited)(/* @__PURE__ */ showIntDictShow(index))();
      };
    });
    return () => {
      const $unit$1 = $action$1();
      return 99 | 0;
    };
  });
})();
export const untilAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      let counter = 0 | 0;
      const $function = Effect.untilE;
      const $effect = () => {
        let count;
        const section300 = counter;
        const $value = section300 + (1 | 0) | 0;
        count = counter = $value;
        const $action = record(visited)(/* @__PURE__ */ showIntDictShow(count));
        const $unit = $action();
        const $action$1 = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(count === (2 | 0))(Effect_Exception.throw("stop at 2"));
        const $unit$1 = $action$1();
        return count >= (5 | 0);
      };
      const $action$2 = $function($effect);
      const $unit$2 = $action$2();
      return counter;
    };
  });
})();
export const whileAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return () => {
      let counter = 0 | 0;
      const $function = Effect.whileE;
      const $function$1 = (section384) => section384 < (5 | 0);
      const $effect = () => {
        return $function$1(counter);
      };
      const $function$2 = $function($effect);
      const $effect$1 = () => {
        let count;
        const section406 = counter;
        const $value = section406 + (1 | 0) | 0;
        count = counter = $value;
        const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(count === (3 | 0))(Effect_Exception.throw("stop at 3"));
        const $unit = $action();
        return record(visited)(/* @__PURE__ */ showIntDictShow(count))();
      };
      return $function$2($effect$1)();
    };
  });
})();
export const foreachAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return Effect.foreachE([
      "a",
      "b",
      "c",
      "d"
    ])((element) => {
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(element === "c")(Effect_Exception.throw("stop at c"));
      return () => {
        const $unit = $action();
        return record(visited)(element)();
      };
    });
  });
})();
export const nestedAborts = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return Effect.forE(0 | 0)(3 | 0)((row) => {
      const $action = record(visited)("row " + /* @__PURE__ */ showIntDictShow(row));
      return () => {
        const $unit = $action();
        return Effect.forE(0 | 0)(3 | 0)((column) => {
          const $action$1 = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(row === (1 | 0) && column === (1 | 0))(Effect_Exception.throw("stop at 1:1"));
          return () => {
            const $unit$1 = $action$1();
            return record(visited)(/* @__PURE__ */ showIntDictShow(row) + (":" + /* @__PURE__ */ showIntDictShow(column)))();
          };
        })();
      };
    });
  });
})();
export const recovers = /* @__PURE__ */ (() => {
  return observe((visited) => {
    return Effect.forE(0 | 0)(4 | 0)((index) => {
      const $function = Effect_Exception.catchException((error) => record(visited)("caught " + Effect_Exception.message(error)));
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(index === (1 | 0))(Effect_Exception.throw("one"));
      const $effect = () => {
        const $unit = $action();
        return record(visited)(/* @__PURE__ */ showIntDictShow(index))();
      };
      return $function($effect);
    });
  });
})();

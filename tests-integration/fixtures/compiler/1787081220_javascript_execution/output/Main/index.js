import * as Library from "../Library/index.js";
import * as $foreign from "./foreign.js";
export const None = "None";
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function readHostile(value) {
  return value["hostile-field"];
}
export function readProto(value) {
  return value.__proto__;
}
export function capture(captured) {
  return ($int) => captured;
}
export function apply($function) {
  return (value) => {
    return $function(value);
  };
}
export function addCaptured(amount) {
  return (value) => addInt(amount)(value);
}
export function nestedJoin(outer) {
  return (inner) => {
    if (outer) {
      let $result;
      $case: {
        if (inner === true) {
          $result = 1 | 0;
          break $case;
        }
        if (inner === false) {
          $result = 2 | 0;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      const result = $result;
      return addInt(foreignValue)(result);
    } else {
      return 0 | 0;
    }
  };
}
export function countdown(value) {
  if (equalInt(value)(0 | 0)) {
    return 0 | 0;
  } else {
    return addInt(1 | 0)(countdown(decrementInt(value)));
  }
}
function $tail_isEven_isOdd($state, $argument0) {
  while (true) {
    switch ($state) {
      // isEven
      case 0: {
        const $currentArgument0 = $argument0;
        if (equalInt($currentArgument0)(0 | 0)) {
          return true;
        } else {
          $argument0 = decrementInt($currentArgument0);
          $state = 1;
          continue;
        }
      }
      // isOdd
      case 1: {
        const $currentArgument0$1 = $argument0;
        if (equalInt($currentArgument0$1)(0 | 0)) {
          return false;
        } else {
          $argument0 = decrementInt($currentArgument0$1);
          $state = 0;
          continue;
        }
      }
    }
  }
}
export function isEven(value) {
  return $tail_isEven_isOdd(0, value);
}
export function isOdd(value$1) {
  return $tail_isEven_isOdd(1, value$1);
}
export function capturedMutual(captured) {
  return (condition) => {
    const $tail_localSecond_localFirst = ($state, $argument0) => {
      while (true) {
        switch ($state) {
          // localSecond
          case 0: {
            const $currentArgument0 = $argument0;
            if ($currentArgument0 === true) {
              return captured;
            }
            if ($currentArgument0 === false) {
              $argument0 = true;
              $state = 1;
              continue;
            }
            throw new Error("Pattern match failure");
          }
          // localFirst
          case 1: {
            const $currentArgument0$1 = $argument0;
            if ($currentArgument0$1 === true) {
              return captured;
            }
            if ($currentArgument0$1 === false) {
              $argument0 = true;
              $state = 0;
              continue;
            }
            throw new Error("Pattern match failure");
          }
        }
      }
    };
    const localSecond = ($boolean) => {
      return $tail_localSecond_localFirst(0, $boolean);
    };
    const localFirst = ($boolean$1) => {
      return $tail_localSecond_localFirst(1, $boolean$1);
    };
    return localFirst(condition);
  };
}
export function first(choice) {
  if (choice === "None") {
    return 0 | 0;
  }
  if (choice.tag === "Pair") {
    const { _1: left } = choice;
    return left;
  }
  throw new Error("Pattern match failure");
}
export function partialPattern($choice) {
  if ($choice.tag === "Pair") {
    const { _1: left } = $choice;
    return left;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function unwrapWrapped($wrapped) {
  if ($wrapped.tag === "Wrapped") {
    const { _1: value } = $wrapped;
    return value;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function measure(dictionary) {
  return dictionary.measure;
}
export const addInt = $foreign["addInt"];
export const decrementInt = $foreign["decrementInt"];
export const equalInt = $foreign["equalInt"];
export const foreignValue = $foreign["foreignValue"];
export const effectValue = $foreign["effectValue"];
const $await = $foreign["await"];
export const integer = 42 | 0;
export const number = 1.5;
export const string = "iris";
export const array = [
  1 | 0,
  2 | 0,
  3 | 0
];
export const model = {
  count: 0 | 0,
  nested: { enabled: true },
  "hostile-field": $await,
  ["__proto__"]: "data, not a prototype"
};
export const updated = /* @__PURE__ */ (() => {
  return {
    ...model,
    count: 1 | 0,
    nested: {
      ...model.nested,
      enabled: false
    }
  };
})();
export const curried = apply(addCaptured(2 | 0))(40 | 0);
export const pair = {
  tag: "Pair",
  _1: 7 | 0,
  _2: 8 | 0
};
export const crossModule = unwrapWrapped(Library.wrapped);
export const forwardReference = Library.forward;
export const measureInt = { measure: addInt(1 | 0) };
export const evidenceValue = /* @__PURE__ */ measure(measureInt)(41 | 0);
export { $await as "await" };

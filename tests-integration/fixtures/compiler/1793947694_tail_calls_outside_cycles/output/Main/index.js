import * as Data_Boolean from "../Data.Boolean/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
export const Literal = ($value0) => ({
  tag: "Literal",
  _1: $value0
});
export const Add = ($value0) => ($value1) => ({
  tag: "Add",
  _1: $value0,
  _2: $value1
});
export const Parenthesised = ($value0) => ({
  tag: "Parenthesised",
  _1: $value0
});
export const Sum = ($value0) => ({
  tag: "Sum",
  _1: $value0
});
export const Nil = "Nil";
export const Cons = ($value0) => ($value1) => ({
  tag: "Cons",
  _1: $value0,
  _2: $value1
});
export function evaluate($expression) {
  let $argument0 = $expression;
  while (true) {
    const $currentArgument0 = $argument0;
    if ($currentArgument0.tag === "Literal") {
      const { _1: value } = $currentArgument0;
      return value;
    }
    if ($currentArgument0.tag === "Add") {
      const { _1: left, _2: right } = $currentArgument0;
      return evaluate(left) + evaluate(right) | 0;
    }
    if ($currentArgument0.tag === "Parenthesised") {
      const { _1: expression } = $currentArgument0;
      $argument0 = expression;
      continue;
    }
    if ($currentArgument0.tag === "Sum") {
      const { _1: terms } = $currentArgument0;
      return sumTerms(0 | 0)(terms);
    }
    throw new Error("Pattern match failure");
  }
}
export function sumTerms($int) {
  return ($terms) => {
    let $argument0 = $int;
    let $argument1 = $terms;
    while (true) {
      const $currentArgument0 = $argument0;
      const $currentArgument1 = $argument1;
      if ($currentArgument1 === "Nil") {
        const total = $currentArgument0;
        return total;
      }
      if ($currentArgument1.tag === "Cons") {
        const total$1 = $currentArgument0;
        const { _1: expression, _2: rest } = $currentArgument1;
        $argument0 = total$1 + evaluate(expression) | 0;
        $argument1 = rest;
        continue;
      }
      throw new Error("Pattern match failure");
    }
  };
}
export function countToZero(counter) {
  const settle = (step) => {
    return () => {
      const count = counter.value;
      if (count < (0 | 0)) {
        const $action = advance(step);
        const $int = $action();
        return count;
      } else {
        return count;
      }
    };
  };
  const advance = (step$1) => {
    return () => {
      let $unit;
      const section160 = counter.value;
      const $value = section160 + step$1 | 0;
      counter.value = $value;
      $unit = Data_Unit.unit;
      return settle(step$1)();
    };
  };
  return advance(1 | 0);
}
export function skipToZero(counter) {
  const skip = (depth) => {
    let $argument0 = depth;
    while (true) {
      const $currentArgument0 = $argument0;
      if ($currentArgument0 > (0 | 0)) {
        $argument0 = $currentArgument0 - (1 | 0) | 0;
        continue;
      }
      if (Data_Boolean.otherwise) {
        return () => {
          let $unit;
          const section263 = counter.value;
          const $value = section263 + (1 | 0) | 0;
          counter.value = $value;
          $unit = Data_Unit.unit;
          return settle($currentArgument0)();
        };
      }
      throw new Error("Pattern match failure");
    }
  };
  const settle = (depth$1) => {
    return () => {
      const count = counter.value;
      if (count < (0 | 0)) {
        const $action = skip(depth$1);
        const $int = $action();
        return count;
      } else {
        return count;
      }
    };
  };
  return skip;
}

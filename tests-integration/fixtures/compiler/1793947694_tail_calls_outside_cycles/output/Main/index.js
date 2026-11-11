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
function $tail_evaluate_sumTerms($state, $argument0, $argument1) {
  while (true) {
    switch ($state) {
      // evaluate
      case 0: {
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
          $argument1 = null;
          $state = 0;
          continue;
        }
        if ($currentArgument0.tag === "Sum") {
          const { _1: terms } = $currentArgument0;
          $argument0 = 0 | 0;
          $argument1 = terms;
          $state = 1;
          continue;
        }
        throw new Error("Pattern match failure");
      }
      // sumTerms
      case 1: {
        const $currentArgument0$1 = $argument0;
        const $currentArgument1 = $argument1;
        if ($currentArgument1 === "Nil") {
          const total = $currentArgument0$1;
          return total;
        }
        if ($currentArgument1.tag === "Cons") {
          const total$1 = $currentArgument0$1;
          const { _1: expression$1, _2: rest } = $currentArgument1;
          $argument0 = total$1 + evaluate(expression$1) | 0;
          $argument1 = rest;
          $state = 1;
          continue;
        }
        throw new Error("Pattern match failure");
      }
    }
  }
}
export function evaluate($expression) {
  return $tail_evaluate_sumTerms(0, $expression, null);
}
export function sumTerms($int) {
  return ($terms) => {
    return $tail_evaluate_sumTerms(1, $int, $terms);
  };
}
export function countToZero(counter) {
  const $tail_settle_advance = ($state, $argument0) => {
    while (true) {
      switch ($state) {
        // settle
        case 0: {
          const $currentArgument0 = $argument0;
          return () => {
            const count = counter.value;
            if (count < (0 | 0)) {
              const $action = advance($currentArgument0);
              const $int = $action();
              return [false, count];
            } else {
              return [false, count];
            }
          };
        }
        // advance
        case 1: {
          const $currentArgument0$1 = $argument0;
          return () => {
            let $unit;
            const section160 = counter.value;
            const $value = section160 + $currentArgument0$1 | 0;
            counter.value = $value;
            $unit = Data_Unit.unit;
            return [
              true,
              0,
              $currentArgument0$1
            ];
          };
        }
      }
    }
  };
  const settle = (step) => {
    const $initialStep = $tail_settle_advance(0, step);
    return () => {
      let $step;
      $step = $initialStep;
      while (true) {
        const $result = $step();
        if (!$result[0]) {
          return $result[1];
        }
        $step = $tail_settle_advance($result[1], $result[2]);
      }
    };
  };
  const advance = (step$1) => {
    const $initialStep$1 = $tail_settle_advance(1, step$1);
    return () => {
      let $step$1;
      $step$1 = $initialStep$1;
      while (true) {
        const $result$1 = $step$1();
        if (!$result$1[0]) {
          return $result$1[1];
        }
        $step$1 = $tail_settle_advance($result$1[1], $result$1[2]);
      }
    };
  };
  return advance(1 | 0);
}
export function skipToZero(counter) {
  const $tail_settle_skip = ($state, $argument0) => {
    while (true) {
      switch ($state) {
        // settle
        case 0: {
          const $currentArgument0 = $argument0;
          return () => {
            const count = counter.value;
            if (count < (0 | 0)) {
              const $action = skip($currentArgument0);
              const $int = $action();
              return [false, count];
            } else {
              return [false, count];
            }
          };
        }
        // skip
        case 1: {
          const $currentArgument0$1 = $argument0;
          if ($currentArgument0$1 > (0 | 0)) {
            $argument0 = $currentArgument0$1 - (1 | 0) | 0;
            $state = 1;
            continue;
          }
          if (Data_Boolean.otherwise) {
            return () => {
              let $unit;
              const section263 = counter.value;
              const $value = section263 + (1 | 0) | 0;
              counter.value = $value;
              $unit = Data_Unit.unit;
              return [
                true,
                0,
                $currentArgument0$1
              ];
            };
          }
          throw new Error("Pattern match failure");
        }
      }
    }
  };
  const settle = (depth) => {
    const $initialStep = $tail_settle_skip(0, depth);
    return () => {
      let $step;
      $step = $initialStep;
      while (true) {
        const $result = $step();
        if (!$result[0]) {
          return $result[1];
        }
        $step = $tail_settle_skip($result[1], $result[2]);
      }
    };
  };
  const skip = (depth$1) => {
    const $initialStep$1 = $tail_settle_skip(1, depth$1);
    return () => {
      let $step$1;
      $step$1 = $initialStep$1;
      while (true) {
        const $result$1 = $step$1();
        if (!$result$1[0]) {
          return $result$1[1];
        }
        $step$1 = $tail_settle_skip($result$1[1], $result$1[2]);
      }
    };
  };
  return skip;
}

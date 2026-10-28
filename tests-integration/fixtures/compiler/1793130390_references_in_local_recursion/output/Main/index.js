import * as Data_Unit from "../Data.Unit/index.js";
export function countdown(start) {
  return () => {
    let steps = 0 | 0;
    const go = (remaining) => {
      const $tail_go = ($state, $argument0) => {
        while (true) {
          const $currentArgument0 = $argument0;
          if ($currentArgument0 > (0 | 0)) {
            return () => {
              steps = steps + $currentArgument0 | 0;
              const $tailArgument = $currentArgument0 - (1 | 0) | 0;
              return [
                true,
                0,
                $tailArgument
              ];
            };
          } else {
            const $value = Data_Unit.unit;
            return () => {
              return [false, $value];
            };
          }
        }
      };
      const $initialStep = $tail_go(0, remaining);
      return () => {
        let $step;
        $step = $initialStep;
        while (true) {
          const $result = $step();
          if (!$result[0]) {
            return $result[1];
          }
          $step = $tail_go($result[1], $result[2]);
        }
      };
    };
    const $action = go(start);
    const $unit = $action();
    const $action$1 = go(2 | 0);
    const $unit$1 = $action$1();
    return steps;
  };
}

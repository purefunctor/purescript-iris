import * as $foreign from "./foreign.js";
export function appendBeforeCase(values) {
  return (count) => {
    const $element = traced("literal")(0 | 0);
    const $element$1 = traced("spread")(values);
    let $result;
    const $scrutinee = traced("scrutinee")(count);
    $case: {
      if ($scrutinee === (0 | 0)) {
        $result = [];
        break $case;
      }
      $result = traced("nonempty")([count]);
      break $case;
    }
    return [
      $element,
      ...$element$1,
      ...$result
    ];
  };
}
export const traced = $foreign["traced"];

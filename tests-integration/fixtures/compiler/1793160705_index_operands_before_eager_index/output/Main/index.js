import * as $foreign from "./foreign.js";
export function indexBeforeCase(values) {
  return (position) => {
    const $value = traced("array")(values);
    let $result;
    const $scrutinee = traced("scrutinee")(position);
    $case: {
      if ($scrutinee === (0 | 0)) {
        $result = traced("first")(0 | 0);
        break $case;
      }
      $result = traced("last")(2 | 0);
      break $case;
    }
    return $value[$result];
  };
}
export const traced = $foreign["traced"];

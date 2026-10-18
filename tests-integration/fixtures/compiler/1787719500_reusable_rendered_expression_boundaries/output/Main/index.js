import * as $foreign from "./foreign.js";
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export function stableApplication($function) {
  return (condition) => {
    let $result;
    $case: {
      if (condition === true) {
        $result = observe("application-then")(1 | 0);
        break $case;
      }
      if (condition === false) {
        $result = observe("application-else")(2 | 0);
        break $case;
      }
      throw new Error("Pattern match failure");
    }
    return $function($result);
  };
}
export function observedApplication(condition) {
  const $function = observed.apply;
  let $result;
  $case: {
    if (condition === true) {
      $result = observe("observed-then")(3 | 0);
      break $case;
    }
    if (condition === false) {
      $result = observe("observed-else")(4 | 0);
      break $case;
    }
    throw new Error("Pattern match failure");
  }
  return $function($result);
}
export function stableArray(value) {
  return (condition) => {
    let $result;
    $case: {
      if (condition === true) {
        $result = observe("array-then")(5 | 0);
        break $case;
      }
      if (condition === false) {
        $result = observe("array-else")(6 | 0);
        break $case;
      }
      throw new Error("Pattern match failure");
    }
    return [value, $result];
  };
}
export function observedArray(condition) {
  const $element = observed.value;
  let $result;
  $case: {
    if (condition === true) {
      $result = observe("observed-array-then")(7 | 0);
      break $case;
    }
    if (condition === false) {
      $result = observe("observed-array-else")(8 | 0);
      break $case;
    }
    throw new Error("Pattern match failure");
  }
  return [$element, $result];
}
export function stablePure(value) {
  return () => {
    return value;
  };
}
export function stableMap($function) {
  const $action = constructEffect("stable-map")(9 | 0);
  return () => {
    return $function($action());
  };
}
export function observedMap($boolean) {
  const $function = observed.apply;
  const $action = constructEffect("observed-map")(10 | 0);
  return () => {
    return $function($action());
  };
}
export function mixedApply($boolean) {
  const $functionAction = constructEffect("mixed-function")((value) => observe("mixed-call")(value));
  const $action = constructEffect("mixed-argument-first")(18 | 0);
  return () => {
    const $function = $functionAction();
    let $argument;
    const value$1 = $action();
    $argument = constructEffect("mixed-argument-second")(value$1)();
    return $function($argument);
  };
}
export function joinedEffect(condition) {
  let $result;
  if (condition) {
    const $function = observed.apply;
    const $action = constructEffect("joined-then")(16 | 0);
    $result = () => {
      return $function($action());
    };
  } else {
    const $function$1 = observed.apply;
    const $action$1 = constructEffect("joined-else")(17 | 0);
    $result = () => {
      return $function$1($action$1());
    };
  }
  return [$result];
}
export function joinedPattern(condition) {
  let $result;
  $case: {
    if (condition === true) {
      $result = {
        tag: "Box",
        _1: observe("pattern-then")(11 | 0)
      };
      break $case;
    }
    if (condition === false) {
      $result = {
        tag: "Box",
        _1: observe("pattern-else")(12 | 0)
      };
      break $case;
    }
    throw new Error("Pattern match failure");
  }
  if ($result.tag === "Box") {
    const { _1: value } = $result;
    return value;
  }
  throw new Error("Pattern match failure");
}
export const constructEffect = $foreign["constructEffect"];
export const observe = $foreign["observe"];
export const observed = $foreign["observed"];

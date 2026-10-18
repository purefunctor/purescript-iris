import * as $foreign from "./foreign.js";
export function updateLocal(source) {
  return {
    ...source,
    first: observe("local-first")(10 | 0),
    nested: {
      ...source.nested,
      value: observe("local-nested")(20 | 0)
    },
    last: observe("local-last")(30 | 0)
  };
}
export function updateCall(shouldThrow) {
  const $record = makeModel("call");
  return {
    ...$record,
    first: observe("call-first")(10 | 0),
    nested: {
      ...$record.nested,
      value: failAt("call-nested")(shouldThrow)(20 | 0)
    },
    last: observe("call-last")(30 | 0)
  };
}
export function updateDeep($boolean) {
  const $record = makeModel("deep");
  return {
    ...$record,
    nested: {
      ...$record.nested,
      value: observe("deep-nested")(50 | 0),
      inner: {
        ...$record.nested.inner,
        value: observe("deep-inner")(60 | 0)
      }
    }
  };
}
export function updateControl(condition) {
  return (source) => {
    const $record = {
      ...source,
      first: observe("control-first")(10 | 0)
    };
    const $record$1 = { ...source.nested };
    let $result;
    $case: {
      if (condition === true) {
        $result = observe("control-then")(30 | 0);
        break $case;
      }
      if (condition === false) {
        $result = observe("control-else")(31 | 0);
        break $case;
      }
      throw new Error("Pattern match failure");
    }
    return {
      ...$record,
      nested: {
        ...$record$1,
        value: $result
      },
      last: observe("control-last")(32 | 0)
    };
  };
}
export function updateOpen(source) {
  return {
    ...source,
    first: observe("open-first")(40 | 0)
  };
}
export const failAt = $foreign["failAt"];
export const makeModel = $foreign["makeModel"];
export const observe = $foreign["observe"];

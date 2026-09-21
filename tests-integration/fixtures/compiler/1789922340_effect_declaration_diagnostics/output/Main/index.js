export const Program = ($value0) => ({
  tag: "Program",
  _1: $value0
});
export function succeed(value) {
  return {
    tag: "Program",
    _1: value
  };
}
export function bind(unionLeftRightCombinedDict) {
  const $closure = ($program) => {
    if ($program.tag === "Program") {
      const { _1: value } = $program;
      return (continuation) => {
        const $scrutinee = continuation(value);
        if ($scrutinee.tag === "Program") {
          const { _1: result } = $scrutinee;
          return {
            tag: "Program",
            _1: result
          };
        }
        throw new Error("Pattern match failure");
      };
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return $closure;
}
export function writeLog(value) {
  return {
    tag: "Program",
    _1: value
  };
}
export function restrictEffects(subsetRequiredAllowedDict) {
  const $closure = ($program) => {
    if ($program.tag === "Program") {
      const { _1: value } = $program;
      return {
        tag: "Program",
        _1: value
      };
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return $closure;
}
export const fabricatedSubset = {};
export const databaseFailure = {
  tag: "Program",
  _1: 0 | 0
};
export const validationFailure = {
  tag: "Program",
  _1: 0 | 0
};
export const missingEffect = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ restrictEffects($result)(/* @__PURE__ */ bind({})({
    tag: "Program",
    _1: 1 | 0
  })((value) => /* @__PURE__ */ bind({})(writeLog(value))((logged) => succeed(logged))));
})();
export const anotherMissingEffect = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ restrictEffects($result)(/* @__PURE__ */ bind({})(writeLog(2 | 0))((logged) => /* @__PURE__ */ bind({})({
    tag: "Program",
    _1: logged
  })((value) => succeed(value))));
})();
export const uncaughtAborts = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ restrictEffects($result)(/* @__PURE__ */ bind({})(databaseFailure)((value) => /* @__PURE__ */ bind({})(validationFailure)((validation) => succeed(validation))));
})();
export const finalAbort = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ restrictEffects($result)(/* @__PURE__ */ bind({})(succeed(1 | 0))((value) => databaseFailure));
})();
export const localBudget = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  const local = /* @__PURE__ */ restrictEffects($result)(/* @__PURE__ */ bind({})(databaseFailure)((value) => succeed(value)));
  return local;
})();
export const siblingBudgets = (() => {
  const $field = /* @__PURE__ */ bind({})(databaseFailure)((value) => succeed(value));
  let $result;
  throw new Error("Generated code reached a source error");
  return {
    allowed: $field,
    rejected: /* @__PURE__ */ restrictEffects($result)(/* @__PURE__ */ bind({})(databaseFailure)((value$1) => succeed(value$1)))
  };
})();

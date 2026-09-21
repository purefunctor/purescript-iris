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
export function discard(unionLeftRightCombinedDict) {
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
export function catchDatabase(removeAbortDatabaseErrorInputOutputDict) {
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
export function catchValidation(removeAbortValidationErrorInputOutputDict) {
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
export function provideDatabase(removeDatabaseInputOutputDict) {
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
export const findUser = {
  tag: "Program",
  _1: 1 | 0
};
export const currentTime = {
  tag: "Program",
  _1: 2 | 0
};
export const databaseFailure = {
  tag: "Program",
  _1: 0 | 0
};
export const validationFailure = {
  tag: "Program",
  _1: 0 | 0
};
export const combined = /* @__PURE__ */ discard({})(currentTime)(($int) => /* @__PURE__ */ bind({})(findUser)((user) => writeLog(user)));
export const reordered = combined;
export const deduplicated = /* @__PURE__ */ bind({})(findUser)((first) => /* @__PURE__ */ discard({})(findUser)(($int) => succeed(first)));
export const duplicateAnnotation = findUser;
export const expanded = /* @__PURE__ */ restrictEffects({})(findUser);
export const handled = /* @__PURE__ */ provideDatabase({})(combined);
export const failures = /* @__PURE__ */ discard({})(databaseFailure)(($int) => validationFailure);
export const partiallyHandled = /* @__PURE__ */ catchDatabase({})(failures);
export const fullyHandled = /* @__PURE__ */ catchValidation({})(/* @__PURE__ */ catchDatabase({})(failures));
export const openTail = /* @__PURE__ */ provideDatabase({});

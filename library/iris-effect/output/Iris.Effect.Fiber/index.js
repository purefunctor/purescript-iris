import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_List from "../Data.List/index.js";
import * as Data_List_Types from "../Data.List.Types/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Effect_Exception from "../Effect.Exception/index.js";
import * as Unsafe_Coerce from "../Unsafe.Coerce/index.js";
import * as $foreign from "./foreign.js";
export const Pure = ($value0) => ({
  tag: "Pure",
  _1: $value0
});
export const Throw = ($value0) => ({
  tag: "Throw",
  _1: $value0
});
export const Sync = ($value0) => ({
  tag: "Sync",
  _1: $value0
});
export const Fallible = ($value0) => ({
  tag: "Fallible",
  _1: $value0
});
export const Run = ($value0) => ({
  tag: "Run",
  _1: $value0
});
export const Async = ($value0) => ({
  tag: "Async",
  _1: $value0
});
export const Bind = ($value0) => ($value1) => ({
  tag: "Bind",
  _1: $value0,
  _2: $value1
});
export const Catch = ($value0) => ($value1) => ({
  tag: "Catch",
  _1: $value0,
  _2: $value1
});
export const Bracket = ($value0) => ($value1) => ($value2) => ({
  tag: "Bracket",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
export const Fork = ($value0) => ($value1) => ({
  tag: "Fork",
  _1: $value0,
  _2: $value1
});
export const Succeeded = ($value0) => ({
  tag: "Succeeded",
  _1: $value0
});
export const Failed = ($value0) => ({
  tag: "Failed",
  _1: $value0
});
export const Died = ($value0) => ({
  tag: "Died",
  _1: $value0
});
const AttemptCatch = ($value0) => ($value1) => ({
  tag: "AttemptCatch",
  _1: $value0,
  _2: $value1
});
const AttemptResume = ($value0) => ($value1) => ({
  tag: "AttemptResume",
  _1: $value0,
  _2: $value1
});
const AttemptBracket = ($value0) => ($value1) => ($value2) => ({
  tag: "AttemptBracket",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
const AttemptRelease = ($value0) => ($value1) => ($value2) => ({
  tag: "AttemptRelease",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
const AttemptFinalizer = ($value0) => ({
  tag: "AttemptFinalizer",
  _1: $value0
});
const AttemptFinalized = ($value0) => ($value1) => ($value2) => ({
  tag: "AttemptFinalized",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
const Suspended = "Suspended";
const Running = "Running";
const Runnable = "Runnable";
const Pending = "Pending";
const Return = "Return";
const Completed = "Completed";
export function bindStep(step) {
  return (continuation) => {
    return {
      tag: "Bind",
      _1: step,
      _2: continuation
    };
  };
}
export function catchStep(step) {
  return (handler) => {
    return {
      tag: "Catch",
      _1: step,
      _2: handler
    };
  };
}
export function bracketStep(acquire) {
  return (conditions) => {
    return (use) => {
      return {
        tag: "Bracket",
        _1: acquire,
        _2: conditions,
        _3: use
      };
    };
  };
}
export function mapStep($function) {
  return (step) => {
    if (step.tag === "Pure") {
      const { _1: value } = step;
      return {
        tag: "Pure",
        _1: $function(value)
      };
    }
    return bindStep(step)((composeArgument) => ({
      tag: "Pure",
      _1: /* @__PURE__ */ $function(composeArgument)
    }));
  };
}
export function forkStep(child) {
  return (continuation) => {
    return {
      tag: "Fork",
      _1: child,
      _2: continuation
    };
  };
}
export function deferStep(thunk) {
  const $value = Data_Unit.unit;
  const $effect = () => {
    let $value$1;
    $value$1 = $value;
    return thunk($value$1);
  };
  return {
    tag: "Run",
    _1: $effect
  };
}
function tryEffect(effect) {
  const $argument = Data_Maybe.Just;
  return () => {
    return tryHost($argument, "Nothing", effect);
  };
}
function enqueue(scheduler) {
  return (thunk) => {
    const $reference = scheduler.back;
    return () => {
      const back = $reference.value;
      const $reference$1 = scheduler.back;
      const $value = {
        tag: "Cons",
        _1: thunk,
        _2: back
      };
      $reference$1.value = $value;
      return drainUnlessDraining(scheduler)();
    };
  };
}
function prioritize(scheduler) {
  return (thunk) => {
    const $reference = scheduler.front;
    return () => {
      const front = $reference.value;
      const $reference$1 = scheduler.front;
      const $value = {
        tag: "Cons",
        _1: thunk,
        _2: front
      };
      $reference$1.value = $value;
      return undefined;
    };
  };
}
function drainUnlessDraining(scheduler) {
  const $reference = scheduler.draining;
  return () => {
    const draining = $reference.value;
    return (/* @__PURE__ */ Control_Applicative.unless(Effect.applicativeEffect)(draining)(drain(scheduler)))();
  };
}
function drain(scheduler) {
  const $reference = scheduler.draining;
  return () => {
    $reference.value = true;
    const $reference$1 = scheduler.escaped;
    $reference$1.value = "Nothing";
    const $function = Effect.untilE;
    const $reference$2 = scheduler.front;
    const $effect = () => {
      const front = $reference$2.value;
      if (front.tag === "Cons") {
        const { _1: thunk, _2: rest } = front;
        const $reference$3 = scheduler.front;
        $reference$3.value = rest;
        const $action = tryEffect(thunk);
        const escaped = $action();
        const $action$1 = recordFirstEscape(scheduler.escaped)(escaped);
        const $unit = $action$1();
        return false;
      }
      if (front === "Nil") {
        const $reference$4 = scheduler.back;
        const back = $reference$4.value;
        if (back === "Nil") {
          return true;
        }
        const $reference$5 = scheduler.back;
        $reference$5.value = "Nil";
        const $reference$6 = scheduler.front;
        const $value = Data_List.reverse(back);
        $reference$6.value = $value;
        return false;
      }
      throw new Error("Pattern match failure");
    };
    const $action$2 = $function($effect);
    const $unit$1 = $action$2();
    const $reference$7 = scheduler.draining;
    $reference$7.value = false;
    return rethrowEscaped(scheduler.escaped)();
  };
}
function recordFirstEscape(recorded) {
  return (escaped) => {
    const $function = Control_Applicative.when(Effect.applicativeEffect)(Data_Maybe.isJust(escaped));
    const $effect = () => {
      const previous = recorded.value;
      const $function$1 = Control_Applicative.when(Effect.applicativeEffect)(Data_Maybe.isNothing(previous));
      const $effect$1 = () => {
        recorded.value = escaped;
        return undefined;
      };
      return (/* @__PURE__ */ $function$1($effect$1))();
    };
    return /* @__PURE__ */ $function($effect);
  };
}
function rethrowEscaped(recorded) {
  return () => {
    const escaped = recorded.value;
    if (escaped.tag === "Just") {
      const { _1: error } = escaped;
      return Effect_Exception.throwException(error)();
    }
    if (escaped === "Nothing") {
      return Data_Unit.unit;
    }
    throw new Error("Pattern match failure");
  };
}
export function makeFiber(runtime) {
  return (step) => {
    return () => {
      const status = { value: "Suspended" };
      const runnableResult = { value: noResult };
      const pendingCanceler = { value: "Nothing" };
      const exit = { value: noExit };
      const $value = opaqueStep(step);
      const current = { value: $value };
      const returnValue = { value: "Nothing" };
      const failure = { value: "Nothing" };
      const defect = { value: "Nothing" };
      const interrupt = { value: "Nothing" };
      const continuations = { value: "Nil" };
      const attempts = { value: "Nil" };
      const bracketCount = { value: 0 | 0 };
      const joins = { value: "Nil" };
      const joinIdentifier = { value: 0 | 0 };
      const runTick = { value: 0 | 0 };
      return {
        runtime,
        status,
        runnableResult,
        pendingCanceler,
        exit,
        current,
        returnValue,
        failure,
        defect,
        interrupt,
        continuations,
        attempts,
        bracketCount,
        joins,
        joinIdentifier,
        runTick
      };
    };
  };
}
export function joinFiber(fiber) {
  return (callback) => {
    const $scrutinee = fiber.runtime;
    const runtime = $scrutinee;
    const $action = onComplete(fiber)(callback);
    return () => {
      const canceler = $action();
      const $reference = fiber.status;
      const status = $reference.value;
      let $result;
      $case: {
        if (status === "Suspended") {
          const $action$1 = prioritize(runtime.scheduler)(run(fiber));
          $result = () => {
            const $unit = $action$1();
            return drainUnlessDraining(runtime.scheduler)();
          };
          break $case;
        }
        const $value = Data_Unit.unit;
        $result = () => {
          return $value;
        };
        break $case;
      }
      const $unit$1 = $result();
      return canceler;
    };
  };
}
export function killFiber(error) {
  return (fiber) => {
    return (callback) => {
      const $reference = fiber.status;
      return () => {
        const status = $reference.value;
        if (status === "Completed") {
          const $unit = callback();
          const $value = Data_Unit.unit;
          return () => {
            return $value;
          };
        }
        const $action = onComplete(fiber)(($exit) => callback);
        const canceler = $action();
        const $reference$1 = fiber.interrupt;
        const $effect = () => {
          const interrupt = $reference$1.value;
          const $function = Control_Applicative.when(Effect.applicativeEffect)(Data_Maybe.isNothing(interrupt));
          const $reference$2 = fiber.interrupt;
          const $value$1 = {
            tag: "Just",
            _1: toOpaque(error)
          };
          const $effect$1 = () => {
            $reference$2.value = $value$1;
            return undefined;
          };
          return (/* @__PURE__ */ $function($effect$1))();
        };
        const markInterrupted = $effect;
        const $reference$3 = fiber.status;
        const $effect$2 = () => {
          $reference$3.value = "Return";
          const $reference$4 = fiber.returnValue;
          $reference$4.value = "Nothing";
          const $reference$5 = fiber.failure;
          $reference$5.value = "Nothing";
          return undefined;
        };
        const abandon = $effect$2;
        let $result;
        $case: {
          if (status === "Suspended") {
            const $reference$6 = fiber.interrupt;
            const $value$2 = {
              tag: "Just",
              _1: toOpaque(error)
            };
            $result = () => {
              $reference$6.value = $value$2;
              const $reference$7 = fiber.exit;
              const $value$3 = {
                tag: "Failed",
                _1: toOpaque(error)
              };
              $reference$7.value = $value$3;
              const $reference$8 = fiber.current;
              $reference$8.value = noStep;
              const $reference$9 = fiber.status;
              $reference$9.value = "Completed";
              return run(fiber)();
            };
            break $case;
          }
          if (status === "Pending") {
            $result = () => {
              const $unit$1 = markInterrupted();
              const $reference$10 = fiber.bracketCount;
              const bracketCount = $reference$10.value;
              const $function$1 = Control_Applicative.when(Effect.applicativeEffect)(bracketCount === (0 | 0));
              const $reference$11 = fiber.pendingCanceler;
              const $effect$3 = () => {
                const pendingCanceler = $reference$11.value;
                const $reference$12 = fiber.pendingCanceler;
                $reference$12.value = "Nothing";
                let $result$1;
                $case$1: {
                  if (pendingCanceler.tag === "Just") {
                    const { _1: pending } = pendingCanceler;
                    $result$1 = pushAttempt(fiber)({
                      tag: "AttemptFinalizer",
                      _1: opaqueStep(pending)
                    });
                    break $case$1;
                  }
                  if (pendingCanceler === "Nothing") {
                    const $value$4 = Data_Unit.unit;
                    $result$1 = () => {
                      return $value$4;
                    };
                    break $case$1;
                  }
                  throw new Error("Pattern match failure");
                }
                const $unit$2 = $result$1();
                const $unit$3 = abandon();
                const $reference$13 = fiber.runTick;
                const runTick = $reference$13.value;
                const $reference$14 = fiber.runTick;
                const $value$5 = runTick + (1 | 0) | 0;
                $reference$14.value = $value$5;
                return run(fiber)();
              };
              return (/* @__PURE__ */ $function$1($effect$3))();
            };
            break $case;
          }
          if (status === "Runnable") {
            $result = () => {
              const $unit$4 = markInterrupted();
              const $reference$15 = fiber.bracketCount;
              const bracketCount$1 = $reference$15.value;
              return (/* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(bracketCount$1 === (0 | 0))(abandon))();
            };
            break $case;
          }
          $result = markInterrupted;
          break $case;
        }
        const $unit$5 = $result();
        return canceler;
      };
    };
  };
}
function onComplete(fiber) {
  return (join) => {
    const $reference = fiber.status;
    return () => {
      const status = $reference.value;
      if (status === "Completed") {
        const $reference$1 = fiber.exit;
        const exit = $reference$1.value;
        const $action = join(exit);
        const $unit = $action();
        const $value = Data_Unit.unit;
        return () => {
          return $value;
        };
      }
      const $reference$2 = fiber.joinIdentifier;
      const identifier = $reference$2.value;
      const $reference$3 = fiber.joinIdentifier;
      const $value$1 = identifier + (1 | 0) | 0;
      $reference$3.value = $value$1;
      const $reference$4 = fiber.joins;
      const joins = $reference$4.value;
      const $reference$5 = fiber.joins;
      const $value$2 = {
        tag: "Cons",
        _1: {
          identifier,
          join
        },
        _2: joins
      };
      $reference$5.value = $value$2;
      const $reference$6 = fiber.joins;
      return () => {
        const remaining = $reference$6.value;
        const $reference$7 = fiber.joins;
        const $value$3 = Data_List.filter((entry) => entry.identifier !== identifier)(remaining);
        $reference$7.value = $value$3;
        return undefined;
      };
    };
  };
}
function pushAttempt(fiber) {
  return (attempt) => {
    const $reference = fiber.attempts;
    return () => {
      const attempts = $reference.value;
      const $reference$1 = fiber.attempts;
      const $value = {
        tag: "Cons",
        _1: attempt,
        _2: attempts
      };
      $reference$1.value = $value;
      return undefined;
    };
  };
}
function enterFinalizer(fiber) {
  const $reference = fiber.returnValue;
  return () => {
    const returnValue = $reference.value;
    const $reference$1 = fiber.failure;
    const failure = $reference$1.value;
    const $reference$2 = fiber.defect;
    const defect = $reference$2.value;
    const $action = pushAttempt(fiber)({
      tag: "AttemptFinalized",
      _1: returnValue,
      _2: failure,
      _3: defect
    });
    const $unit = $action();
    const $reference$3 = fiber.failure;
    $reference$3.value = "Nothing";
    const $reference$4 = fiber.defect;
    $reference$4.value = "Nothing";
    const $reference$5 = fiber.bracketCount;
    const bracketCount = $reference$5.value;
    const $reference$6 = fiber.bracketCount;
    const $value = bracketCount + (1 | 0) | 0;
    $reference$6.value = $value;
    return undefined;
  };
}
function die(fiber) {
  return (error) => {
    const $reference = fiber.defect;
    const $value = {
      tag: "Just",
      _1: error
    };
    return () => {
      $reference.value = $value;
      const $reference$1 = fiber.status;
      $reference$1.value = "Return";
      const $reference$2 = fiber.failure;
      $reference$2.value = "Nothing";
      const $reference$3 = fiber.returnValue;
      $reference$3.value = "Nothing";
      return undefined;
    };
  };
}
function suspendContinuations(fiber) {
  const $reference = fiber.interrupt;
  return () => {
    const interrupt = $reference.value;
    const $reference$1 = fiber.continuations;
    const continuations = $reference$1.value;
    let $result;
    $case: {
      if (continuations === "Nil") {
        const $value = Data_Unit.unit;
        $result = () => {
          return $value;
        };
        break $case;
      }
      $result = pushAttempt(fiber)({
        tag: "AttemptResume",
        _1: continuations,
        _2: Data_Maybe.isJust(interrupt)
      });
      break $case;
    }
    const $unit = $result();
    const $reference$2 = fiber.continuations;
    $reference$2.value = "Nil";
    return undefined;
  };
}
function $tail_run_runFromStatus_runSteps($state, $argument0) {
  while (true) {
    switch ($state) {
      // run
      case 0: {
        const $currentArgument0 = $argument0;
        const $function = Effect.untilE;
        const $action = tryEffect(runFromStatus($currentArgument0));
        const $effect = () => {
          const escaped = $action();
          if (escaped.tag === "Just") {
            const { _1: defect } = escaped;
            const $action$1 = die($currentArgument0)(defect);
            const $unit = $action$1();
            const $reference = $currentArgument0.runTick;
            const runTick = $reference.value;
            const $reference$1 = $currentArgument0.runTick;
            const $value = runTick + (1 | 0) | 0;
            $reference$1.value = $value;
            return false;
          }
          if (escaped === "Nothing") {
            return true;
          }
          throw new Error("Pattern match failure");
        };
        const $action$2 = $function($effect);
        return () => {
          const $unit$1 = $action$2();
          const $reference$2 = $currentArgument0.status;
          const status = $reference$2.value;
          if (status === "Completed") {
            const $reference$3 = $currentArgument0.exit;
            const exit = $reference$3.value;
            return complete($currentArgument0)(exit)();
          }
          return Data_Unit.unit;
        };
      }
      // runFromStatus
      case 1: {
        const $currentArgument0$1 = $argument0;
        const $reference$4 = $currentArgument0$1.status;
        return () => {
          const status$1 = $reference$4.value;
          if (status$1 === "Suspended") {
            const $reference$5 = $currentArgument0$1.status;
            $reference$5.value = "Running";
            return [
              true,
              2,
              $currentArgument0$1
            ];
          }
          if (status$1 === "Runnable") {
            const $reference$6 = $currentArgument0$1.status;
            $reference$6.value = "Running";
            const $reference$7 = $currentArgument0$1.runnableResult;
            const result = $reference$7.value;
            const $reference$8 = $currentArgument0$1.runnableResult;
            $reference$8.value = noResult;
            const $action$3 = deliver($currentArgument0$1)(result);
            const stop = $action$3();
            return [false, (/* @__PURE__ */ Control_Applicative.unless(Effect.applicativeEffect)(stop)(runSteps($currentArgument0$1)))()];
          }
          if (status$1 === "Return") {
            const $reference$9 = $currentArgument0$1.status;
            $reference$9.value = "Running";
            const $action$4 = returnStep($currentArgument0$1);
            const stop$1 = $action$4();
            return [false, (/* @__PURE__ */ Control_Applicative.unless(Effect.applicativeEffect)(stop$1)(runSteps($currentArgument0$1)))()];
          }
          if (status$1 === "Running") {
            return [false, Data_Unit.unit];
          }
          if (status$1 === "Pending") {
            return [false, Data_Unit.unit];
          }
          if (status$1 === "Completed") {
            return [false, Data_Unit.unit];
          }
          throw new Error("Pattern match failure");
        };
      }
      // runSteps
      case 2: {
        const $currentArgument0$2 = $argument0;
        const $function$1 = Effect.untilE;
        const $reference$10 = $currentArgument0$2.current;
        const $effect$1 = () => {
          const step = $reference$10.value;
          if (step.tag === "Bind") {
            const { _1: inner, _2: continuation } = step;
            if (inner.tag === "Pure") {
              const { _1: value } = inner;
              return continueWith($currentArgument0$2)(continuation(value))();
            }
            if (inner.tag === "Sync") {
              const { _1: effect } = inner;
              const value$1 = effect();
              return continueWith($currentArgument0$2)(continuation(value$1))();
            }
            if (inner.tag === "Fallible") {
              const { _1: effect$1 } = inner;
              const result$1 = effect$1();
              if (result$1.tag === "Left") {
                const { _1: error } = result$1;
                return failWith($currentArgument0$2)(error)();
              }
              if (result$1.tag === "Right") {
                const { _1: value$2 } = result$1;
                return continueWith($currentArgument0$2)(continuation(value$2))();
              }
              throw new Error("Pattern match failure");
            }
            if (inner.tag === "Throw") {
              const { _1: error$1 } = inner;
              return failWith($currentArgument0$2)(error$1)();
            }
            if (inner.tag === "Run") {
              const { _1: effect$2 } = inner;
              const next = effect$2();
              if (next.tag === "Pure") {
                const { _1: value$3 } = next;
                return continueWith($currentArgument0$2)(continuation(value$3))();
              }
              if (next.tag === "Sync") {
                const { _1: nextEffect } = next;
                const value$4 = nextEffect();
                return continueWith($currentArgument0$2)(continuation(value$4))();
              }
              if (next.tag === "Fallible") {
                const { _1: nextEffect$1 } = next;
                const result$2 = nextEffect$1();
                if (result$2.tag === "Left") {
                  const { _1: error$2 } = result$2;
                  return failWith($currentArgument0$2)(error$2)();
                }
                if (result$2.tag === "Right") {
                  const { _1: value$5 } = result$2;
                  return continueWith($currentArgument0$2)(continuation(value$5))();
                }
                throw new Error("Pattern match failure");
              }
              if (next.tag === "Throw") {
                const { _1: error$3 } = next;
                return failWith($currentArgument0$2)(error$3)();
              }
              const $action$5 = pushContinuation($currentArgument0$2)(continuation);
              const $unit$2 = $action$5();
              return continueWith($currentArgument0$2)(next)();
            }
            const $action$6 = pushContinuation($currentArgument0$2)(continuation);
            const $unit$3 = $action$6();
            return continueWith($currentArgument0$2)(inner)();
          }
          if (step.tag === "Pure") {
            const { _1: value$6 } = step;
            return succeedWith($currentArgument0$2)(value$6)();
          }
          if (step.tag === "Sync") {
            const { _1: effect$3 } = step;
            const value$7 = effect$3();
            return succeedWith($currentArgument0$2)(value$7)();
          }
          if (step.tag === "Fallible") {
            const { _1: effect$4 } = step;
            const result$3 = effect$4();
            if (result$3.tag === "Left") {
              const { _1: error$4 } = result$3;
              return failWith($currentArgument0$2)(error$4)();
            }
            if (result$3.tag === "Right") {
              const { _1: value$8 } = result$3;
              return succeedWith($currentArgument0$2)(value$8)();
            }
            throw new Error("Pattern match failure");
          }
          if (step.tag === "Throw") {
            const { _1: error$5 } = step;
            return failWith($currentArgument0$2)(error$5)();
          }
          if (step.tag === "Run") {
            const { _1: effect$5 } = step;
            const next$1 = effect$5();
            return continueWith($currentArgument0$2)(next$1)();
          }
          if (step.tag === "Async") {
            const { _1: register } = step;
            const $reference$11 = $currentArgument0$2.runTick;
            const localRunTick = $reference$11.value;
            const $reference$12 = $currentArgument0$2.status;
            $reference$12.value = "Pending";
            const $reference$13 = $currentArgument0$2.pendingCanceler;
            $reference$13.value = "Nothing";
            const $scrutinee = $currentArgument0$2.runtime;
            const runtime = $scrutinee;
            const $action$7 = register((result$4) => {
              const $reference$14 = $currentArgument0$2.runTick;
              return () => {
                const tick = $reference$14.value;
                const $function$2 = Control_Applicative.when(Effect.applicativeEffect)(tick === localRunTick);
                const $reference$15 = $currentArgument0$2.runTick;
                const $value$1 = tick + (1 | 0) | 0;
                const $effect$2 = () => {
                  $reference$15.value = $value$1;
                  const $function$3 = enqueue(runtime.scheduler);
                  const $reference$16 = $currentArgument0$2.runTick;
                  const $effect$3 = () => {
                    const resumeTick = $reference$16.value;
                    const $function$4 = Control_Applicative.when(Effect.applicativeEffect)(resumeTick === (localRunTick + (1 | 0) | 0));
                    const $reference$17 = $currentArgument0$2.pendingCanceler;
                    const $effect$4 = () => {
                      $reference$17.value = "Nothing";
                      const $reference$18 = $currentArgument0$2.runnableResult;
                      $reference$18.value = result$4;
                      const $reference$19 = $currentArgument0$2.status;
                      $reference$19.value = "Runnable";
                      return run($currentArgument0$2)();
                    };
                    return (/* @__PURE__ */ $function$4($effect$4))();
                  };
                  return $function$3($effect$3)();
                };
                return (/* @__PURE__ */ $function$2($effect$2))();
              };
            });
            const canceler = $action$7();
            const $reference$20 = $currentArgument0$2.status;
            const status$2 = $reference$20.value;
            let $result;
            $case: {
              if (status$2 === "Pending") {
                const $reference$21 = $currentArgument0$2.pendingCanceler;
                $result = () => {
                  const pendingCanceler = $reference$21.value;
                  const $function$5 = Control_Applicative.when(Effect.applicativeEffect)(Data_Maybe.isNothing(pendingCanceler));
                  const $reference$22 = $currentArgument0$2.pendingCanceler;
                  const $value$2 = {
                    tag: "Just",
                    _1: canceler
                  };
                  const $effect$5 = () => {
                    $reference$22.value = $value$2;
                    return undefined;
                  };
                  return (/* @__PURE__ */ $function$5($effect$5))();
                };
                break $case;
              }
              const $value$3 = Data_Unit.unit;
              $result = () => {
                return $value$3;
              };
              break $case;
            }
            const $unit$4 = $result();
            return true;
          }
          if (step.tag === "Catch") {
            const { _1: inner$1, _2: handler } = step;
            const $reference$23 = $currentArgument0$2.interrupt;
            const interrupt = $reference$23.value;
            const $action$8 = suspendContinuations($currentArgument0$2);
            const $unit$5 = $action$8();
            const $action$9 = pushAttempt($currentArgument0$2)({
              tag: "AttemptCatch",
              _1: handler,
              _2: Data_Maybe.isJust(interrupt)
            });
            const $unit$6 = $action$9();
            return continueWith($currentArgument0$2)(opaqueStep(inner$1))();
          }
          if (step.tag === "Bracket") {
            const { _1: acquire, _2: conditions, _3: use } = step;
            const $reference$24 = $currentArgument0$2.bracketCount;
            const bracketCount = $reference$24.value;
            const $reference$25 = $currentArgument0$2.bracketCount;
            const $value$4 = bracketCount + (1 | 0) | 0;
            $reference$25.value = $value$4;
            const $reference$26 = $currentArgument0$2.interrupt;
            const interrupt$1 = $reference$26.value;
            const $action$10 = suspendContinuations($currentArgument0$2);
            const $unit$7 = $action$10();
            const $action$11 = pushAttempt($currentArgument0$2)({
              tag: "AttemptBracket",
              _1: conditions,
              _2: use,
              _3: Data_Maybe.isJust(interrupt$1)
            });
            const $unit$8 = $action$11();
            return continueWith($currentArgument0$2)(acquire)();
          }
          if (step.tag === "Fork") {
            const { _1: child, _2: continuation$1 } = step;
            const $action$12 = makeFiber($currentArgument0$2.runtime)(child);
            const forked = $action$12();
            const $scrutinee$1 = $currentArgument0$2.runtime;
            const runtime$1 = $scrutinee$1;
            const $reference$27 = $currentArgument0$2.runnableResult;
            const $value$5 = {
              tag: "Right",
              _1: continuation$1(forked)
            };
            $reference$27.value = $value$5;
            const $reference$28 = $currentArgument0$2.status;
            $reference$28.value = "Runnable";
            const $action$13 = prioritize(runtime$1.scheduler)(run($currentArgument0$2));
            const $unit$9 = $action$13();
            const $action$14 = prioritize(runtime$1.scheduler)(run(forked));
            const $unit$10 = $action$14();
            const $action$15 = drainUnlessDraining(runtime$1.scheduler);
            const $unit$11 = $action$15();
            return true;
          }
          throw new Error("Pattern match failure");
        };
        const $effect$6 = $function$1($effect$1);
        return () => {
          return [false, $effect$6()];
        };
      }
    }
  }
}
function run(fiber) {
  return $tail_run_runFromStatus_runSteps(0, fiber);
}
function runFromStatus(fiber$1) {
  const $initialStep = $tail_run_runFromStatus_runSteps(1, fiber$1);
  return () => {
    let $step;
    $step = $initialStep;
    while (true) {
      const $result$1 = $step();
      if (!$result$1[0]) {
        return $result$1[1];
      }
      $step = $tail_run_runFromStatus_runSteps($result$1[1], $result$1[2]);
    }
  };
}
function runSteps(fiber$2) {
  const $initialStep$1 = $tail_run_runFromStatus_runSteps(2, fiber$2);
  return () => {
    let $step$1;
    $step$1 = $initialStep$1;
    while (true) {
      const $result$2 = $step$1();
      if (!$result$2[0]) {
        return $result$2[1];
      }
      $step$1 = $tail_run_runFromStatus_runSteps($result$2[1], $result$2[2]);
    }
  };
}
function deliver(fiber) {
  return (result) => {
    if (result.tag === "Left") {
      const { _1: error } = result;
      return failWith(fiber)(error);
    }
    if (result.tag === "Right") {
      const { _1: value } = result;
      return succeedWith(fiber)(value);
    }
    throw new Error("Pattern match failure");
  };
}
function succeedWith(fiber) {
  return (value) => {
    const $reference = fiber.continuations;
    return () => {
      const continuations = $reference.value;
      if (continuations.tag === "Cons") {
        const { _1: continuation, _2: rest } = continuations;
        const $reference$1 = fiber.continuations;
        $reference$1.value = rest;
        const $reference$2 = fiber.current;
        const $value = continuation(value);
        $reference$2.value = $value;
        return false;
      }
      if (continuations === "Nil") {
        const $reference$3 = fiber.returnValue;
        const $value$1 = {
          tag: "Just",
          _1: value
        };
        $reference$3.value = $value$1;
        return returnStep(fiber)();
      }
      throw new Error("Pattern match failure");
    };
  };
}
function failWith(fiber) {
  return (error) => {
    const $reference = fiber.failure;
    const $value = {
      tag: "Just",
      _1: error
    };
    return () => {
      $reference.value = $value;
      const $reference$1 = fiber.returnValue;
      $reference$1.value = "Nothing";
      return returnStep(fiber)();
    };
  };
}
function continueWith(fiber) {
  return (step) => {
    const $reference = fiber.current;
    return () => {
      $reference.value = step;
      return false;
    };
  };
}
function pushContinuation(fiber) {
  return (continuation) => {
    const $reference = fiber.continuations;
    return () => {
      const continuations = $reference.value;
      const $reference$1 = fiber.continuations;
      const $value = {
        tag: "Cons",
        _1: continuation,
        _2: continuations
      };
      $reference$1.value = $value;
      return undefined;
    };
  };
}
function returnStep(fiber) {
  const $reference = fiber.continuations;
  return () => {
    $reference.value = "Nil";
    const $function = Effect.untilE;
    const $reference$1 = fiber.attempts;
    const $effect = () => {
      const attempts = $reference$1.value;
      if (attempts === "Nil") {
        const $reference$2 = fiber.defect;
        const defect = $reference$2.value;
        const $reference$3 = fiber.interrupt;
        const interrupt = $reference$3.value;
        const $reference$4 = fiber.failure;
        const failure = $reference$4.value;
        const $reference$5 = fiber.returnValue;
        const returnValue = $reference$5.value;
        let $result;
        $case: {
          if (defect.tag === "Just") {
            const { _1: error } = defect;
            $result = {
              tag: "Died",
              _1: error
            };
            break $case;
          }
          if (interrupt.tag === "Just") {
            const { _1: error$1 } = interrupt;
            $result = {
              tag: "Failed",
              _1: error$1
            };
            break $case;
          }
          if (failure.tag === "Just") {
            const { _1: error$2 } = failure;
            $result = {
              tag: "Failed",
              _1: error$2
            };
            break $case;
          }
          $result = {
            tag: "Succeeded",
            _1: orUnit(returnValue)
          };
          break $case;
        }
        const result = $result;
        const $reference$6 = fiber.exit;
        $reference$6.value = result;
        const $reference$7 = fiber.current;
        $reference$7.value = noStep;
        const $reference$8 = fiber.status;
        $reference$8.value = "Completed";
        return true;
      }
      if (attempts.tag === "Cons") {
        const { _1: attempt, _2: rest } = attempts;
        const $reference$9 = fiber.attempts;
        $reference$9.value = rest;
        const $reference$10 = fiber.interrupt;
        const interrupt$1 = $reference$10.value;
        const $reference$11 = fiber.bracketCount;
        const bracketCount = $reference$11.value;
        const $reference$12 = fiber.failure;
        const failure$1 = $reference$12.value;
        const $reference$13 = fiber.defect;
        const defect$1 = $reference$13.value;
        const interruptedSince = (enqueuedInterrupted) => {
          return Data_Maybe.isJust(interrupt$1) && !enqueuedInterrupted;
        };
        const killed = (enqueuedInterrupted$1) => {
          return interruptedSince(enqueuedInterrupted$1) && bracketCount === (0 | 0);
        };
        if (attempt.tag === "AttemptCatch") {
          const { _1: handler, _2: enqueuedInterrupted$2 } = attempt;
          if (failure$1.tag === "Just") {
            const { _1: error$3 } = failure$1;
            if (!killed(enqueuedInterrupted$2)) {
              const $reference$14 = fiber.failure;
              $reference$14.value = "Nothing";
              const $reference$15 = fiber.current;
              const $value = handler(error$3);
              $reference$15.value = $value;
              return true;
            }
          }
          return false;
        }
        if (attempt.tag === "AttemptResume") {
          const { _1: continuations, _2: enqueuedInterrupted$3 } = attempt;
          if (killed(enqueuedInterrupted$3) || Data_Maybe.isJust(failure$1) || Data_Maybe.isJust(defect$1)) {
            return false;
          } else {
            const $reference$16 = fiber.returnValue;
            const returnValue$1 = $reference$16.value;
            if (continuations.tag === "Cons") {
              const { _1: continuation, _2: others } = continuations;
              const $reference$17 = fiber.continuations;
              $reference$17.value = others;
              const $reference$18 = fiber.current;
              const $value$1 = continuation(orUnit(returnValue$1));
              $reference$18.value = $value$1;
              return true;
            }
            if (continuations === "Nil") {
              return false;
            }
            throw new Error("Pattern match failure");
          }
        }
        if (attempt.tag === "AttemptBracket") {
          const { _1: conditions, _2: use, _3: enqueuedInterrupted$4 } = attempt;
          const $reference$19 = fiber.bracketCount;
          const $value$2 = bracketCount - (1 | 0) | 0;
          $reference$19.value = $value$2;
          if (Data_Maybe.isJust(failure$1) || Data_Maybe.isJust(defect$1)) {
            return false;
          } else {
            const $reference$20 = fiber.returnValue;
            const returnValue$2 = $reference$20.value;
            const resource = orUnit(returnValue$2);
            const $action = pushAttempt(fiber)({
              tag: "AttemptRelease",
              _1: conditions,
              _2: resource,
              _3: enqueuedInterrupted$4
            });
            const $unit = $action();
            if (!interruptedSince(enqueuedInterrupted$4) || (bracketCount - (1 | 0) | 0) > (0 | 0)) {
              const $reference$21 = fiber.current;
              const $value$3 = use(resource);
              $reference$21.value = $value$3;
              return true;
            } else {
              return false;
            }
          }
        }
        if (attempt.tag === "AttemptRelease") {
          const { _1: conditions$1, _2: resource$1, _3: enqueuedInterrupted$5 } = attempt;
          const $reference$22 = fiber.returnValue;
          const returnValue$3 = $reference$22.value;
          let $result$1;
          $case$1: {
            {
              if (killed(enqueuedInterrupted$5)) {
                $result$1 = conditions$1.killed(resource$1);
                break $case$1;
              }
            }
            if (defect$1.tag === "Just") {
              const { _1: error$4 } = defect$1;
              $result$1 = conditions$1.died(error$4)(resource$1);
              break $case$1;
            }
            if (failure$1.tag === "Just") {
              const { _1: error$5 } = failure$1;
              $result$1 = conditions$1.failed(error$5)(resource$1);
              break $case$1;
            }
            $result$1 = conditions$1.completed(orUnit(returnValue$3))(resource$1);
            break $case$1;
          }
          const finalizer = $result$1;
          const $action$1 = enterFinalizer(fiber);
          const $unit$1 = $action$1();
          const $reference$23 = fiber.current;
          const $value$4 = opaqueStep(finalizer);
          $reference$23.value = $value$4;
          return true;
        }
        if (attempt.tag === "AttemptFinalizer") {
          const { _1: finalizer$1 } = attempt;
          const $action$2 = enterFinalizer(fiber);
          const $unit$2 = $action$2();
          const $reference$24 = fiber.current;
          $reference$24.value = finalizer$1;
          return true;
        }
        if (attempt.tag === "AttemptFinalized") {
          const { _1: savedValue, _2: savedFailure, _3: savedDefect } = attempt;
          const $reference$25 = fiber.bracketCount;
          const $value$5 = bracketCount - (1 | 0) | 0;
          $reference$25.value = $value$5;
          let $result$2;
          if (Data_Maybe.isJust(savedDefect)) {
            const $reference$26 = fiber.defect;
            $result$2 = () => {
              $reference$26.value = savedDefect;
              const $reference$27 = fiber.returnValue;
              $reference$27.value = "Nothing";
              const $reference$28 = fiber.failure;
              $reference$28.value = "Nothing";
              return undefined;
            };
          } else {
            if (Data_Maybe.isJust(defect$1)) {
              const $reference$29 = fiber.returnValue;
              $result$2 = () => {
                $reference$29.value = "Nothing";
                const $reference$30 = fiber.failure;
                $reference$30.value = "Nothing";
                return undefined;
              };
            } else {
              if (Data_Maybe.isNothing(savedFailure) && Data_Maybe.isJust(failure$1)) {
                const $reference$31 = fiber.returnValue;
                $result$2 = () => {
                  $reference$31.value = "Nothing";
                  return undefined;
                };
              } else {
                const $reference$32 = fiber.returnValue;
                $result$2 = () => {
                  $reference$32.value = savedValue;
                  const $reference$33 = fiber.failure;
                  $reference$33.value = savedFailure;
                  return undefined;
                };
              }
            }
          }
          const $unit$3 = $result$2();
          return false;
        }
        throw new Error("Pattern match failure");
      }
      throw new Error("Pattern match failure");
    };
    const $action$3 = $function($effect);
    const $unit$4 = $action$3();
    const $reference$34 = fiber.status;
    const status = $reference$34.value;
    if (status === "Completed") {
      return true;
    }
    return false;
  };
}
function complete(fiber) {
  return (exit) => {
    const $reference = fiber.joins;
    return () => {
      const joins = $reference.value;
      const $reference$1 = fiber.joins;
      $reference$1.value = "Nil";
      const escaped = { value: "Nothing" };
      const $value = Data_List.reverse(joins);
      let remaining = $value;
      const $function = Effect.untilE;
      const $effect = () => {
        const pending = remaining;
        if (pending.tag === "Cons") {
          const { _1: entry, _2: rest } = pending;
          remaining = rest;
          const $action = tryEffect(entry.join(exit));
          const outcome = $action();
          const $action$1 = recordFirstEscape(escaped)(outcome);
          const $unit = $action$1();
          return false;
        }
        if (pending === "Nil") {
          return true;
        }
        throw new Error("Pattern match failure");
      };
      const $action$2 = $function($effect);
      const $unit$1 = $action$2();
      return rethrowEscaped(escaped)();
    };
  };
}
const tryHost = $foreign["tryHost"];
const toOpaque = Unsafe_Coerce.unsafeCoerce;
const opaqueStep = Unsafe_Coerce.unsafeCoerce;
export const newRuntime = /* @__PURE__ */ (() => {
  return () => {
    const front = { value: "Nil" };
    const back = { value: "Nil" };
    const draining = { value: false };
    const escaped = { value: "Nothing" };
    return { scheduler: {
      front,
      back,
      draining,
      escaped
    } };
  };
})();
const noResult = {
  tag: "Right",
  _1: toOpaque(Data_Unit.unit)
};
const noExit = {
  tag: "Succeeded",
  _1: toOpaque(Data_Unit.unit)
};
const noStep = {
  tag: "Pure",
  _1: toOpaque(Data_Unit.unit)
};
const orUnit = Data_Maybe.fromMaybe(toOpaque(Data_Unit.unit));

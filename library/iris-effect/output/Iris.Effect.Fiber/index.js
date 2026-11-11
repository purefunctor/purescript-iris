import * as Data_List from "../Data.List/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
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
  return {
    tag: "Run",
    _1: () => {
      let $value$1;
      $value$1 = $value;
      return thunk($value$1);
    }
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
      return Data_Unit.unit;
    };
  };
}
function drainUnlessDraining(scheduler) {
  const $reference = scheduler.draining;
  return () => {
    const draining = $reference.value;
    const $condition = !draining;
    const $action = drain(scheduler);
    if ($condition) {
      $action();
    }
    return Data_Unit.unit;
  };
}
function drain(scheduler) {
  const $reference = scheduler.draining;
  return () => {
    $reference.value = true;
    const $reference$1 = scheduler.escaped;
    $reference$1.value = "Nothing";
    const $reference$2 = scheduler.front;
    while (true) {
      let $condition;
      const front = $reference$2.value;
      $case: {
        if (front.tag === "Cons") {
          const { _1: thunk, _2: rest } = front;
          const $reference$3 = scheduler.front;
          $reference$3.value = rest;
          const $action = tryEffect(thunk);
          const escaped = $action();
          const $action$1 = recordFirstEscape(scheduler.escaped)(escaped);
          const $unit = $action$1();
          $condition = false;
          break $case;
        }
        if (front === "Nil") {
          const $reference$4 = scheduler.back;
          const back = $reference$4.value;
          if (back === "Nil") {
            $condition = true;
            break $case;
          }
          const $reference$5 = scheduler.back;
          $reference$5.value = "Nil";
          const $reference$6 = scheduler.front;
          const $value = Data_List.reverse(back);
          $reference$6.value = $value;
          $condition = false;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      if ($condition) {
        break;
      }
    }
    const $reference$7 = scheduler.draining;
    $reference$7.value = false;
    return rethrowEscaped(scheduler.escaped)();
  };
}
function recordFirstEscape(recorded) {
  return (escaped) => {
    const $condition = Data_Maybe.isJust(escaped);
    return () => {
      if ($condition) {
        const previous = recorded.value;
        const $condition$1 = Data_Maybe.isNothing(previous);
        if ($condition$1) {
          recorded.value = escaped;
        }
      }
      return Data_Unit.unit;
    };
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
      $case: {
        if (status === "Suspended") {
          const $action$1 = prioritize(runtime.scheduler)(run(fiber));
          const $unit = $action$1();
          drainUnlessDraining(runtime.scheduler)();
          break $case;
        }
        break $case;
      }
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
        const markInterrupted = () => {
          const interrupt = $reference$1.value;
          const $condition = Data_Maybe.isNothing(interrupt);
          const $reference$2 = fiber.interrupt;
          const $value$1 = {
            tag: "Just",
            _1: toOpaque(error)
          };
          if ($condition) {
            $reference$2.value = $value$1;
          }
          return Data_Unit.unit;
        };
        const $reference$3 = fiber.status;
        const abandon = () => {
          $reference$3.value = "Return";
          const $reference$4 = fiber.returnValue;
          $reference$4.value = "Nothing";
          const $reference$5 = fiber.failure;
          $reference$5.value = "Nothing";
          return Data_Unit.unit;
        };
        $case: {
          if (status === "Suspended") {
            const $reference$6 = fiber.interrupt;
            const $value$2 = {
              tag: "Just",
              _1: toOpaque(error)
            };
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
            run(fiber)();
            break $case;
          }
          if (status === "Pending") {
            const $unit$1 = markInterrupted();
            const $reference$10 = fiber.bracketCount;
            const bracketCount = $reference$10.value;
            const $condition$1 = bracketCount === (0 | 0);
            const $reference$11 = fiber.pendingCanceler;
            if ($condition$1) {
              const pendingCanceler = $reference$11.value;
              const $reference$12 = fiber.pendingCanceler;
              $reference$12.value = "Nothing";
              $case$1: {
                if (pendingCanceler.tag === "Just") {
                  const { _1: pending } = pendingCanceler;
                  pushAttempt(fiber)({
                    tag: "AttemptFinalizer",
                    _1: opaqueStep(pending)
                  })();
                  break $case$1;
                }
                if (pendingCanceler === "Nothing") {
                  break $case$1;
                }
                throw new Error("Pattern match failure");
              }
              const $unit$2 = abandon();
              const $reference$13 = fiber.runTick;
              const runTick = $reference$13.value;
              const $reference$14 = fiber.runTick;
              const $value$4 = runTick + (1 | 0) | 0;
              $reference$14.value = $value$4;
              run(fiber)();
            }
            break $case;
          }
          if (status === "Runnable") {
            const $unit$3 = markInterrupted();
            const $reference$15 = fiber.bracketCount;
            const bracketCount$1 = $reference$15.value;
            const $condition$2 = bracketCount$1 === (0 | 0);
            if ($condition$2) {
              abandon();
            }
            break $case;
          }
          markInterrupted();
          break $case;
        }
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
        return Data_Unit.unit;
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
      return Data_Unit.unit;
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
    return Data_Unit.unit;
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
      return Data_Unit.unit;
    };
  };
}
function suspendContinuations(fiber) {
  const $reference = fiber.interrupt;
  return () => {
    const interrupt = $reference.value;
    const $reference$1 = fiber.continuations;
    const continuations = $reference$1.value;
    $case: {
      if (continuations === "Nil") {
        break $case;
      }
      pushAttempt(fiber)({
        tag: "AttemptResume",
        _1: continuations,
        _2: Data_Maybe.isJust(interrupt)
      })();
      break $case;
    }
    const $reference$2 = fiber.continuations;
    $reference$2.value = "Nil";
    return Data_Unit.unit;
  };
}
function run(fiber) {
  const $action = tryEffect(runFromStatus(fiber));
  return () => {
    while (true) {
      let $condition;
      const escaped = $action();
      $case: {
        if (escaped.tag === "Just") {
          const { _1: defect } = escaped;
          const $action$1 = die(fiber)(defect);
          const $unit = $action$1();
          const $reference = fiber.runTick;
          const runTick = $reference.value;
          const $reference$1 = fiber.runTick;
          const $value = runTick + (1 | 0) | 0;
          $reference$1.value = $value;
          $condition = false;
          break $case;
        }
        if (escaped === "Nothing") {
          $condition = true;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      if ($condition) {
        break;
      }
    }
    const $reference$2 = fiber.status;
    const status = $reference$2.value;
    if (status === "Completed") {
      const $reference$3 = fiber.exit;
      const exit = $reference$3.value;
      return complete(fiber)(exit)();
    }
    return Data_Unit.unit;
  };
}
function runFromStatus(fiber) {
  const $reference = fiber.status;
  return () => {
    const status = $reference.value;
    if (status === "Suspended") {
      const $reference$1 = fiber.status;
      $reference$1.value = "Running";
      return runSteps(fiber)();
    }
    if (status === "Runnable") {
      const $reference$2 = fiber.status;
      $reference$2.value = "Running";
      const $reference$3 = fiber.runnableResult;
      const result = $reference$3.value;
      const $reference$4 = fiber.runnableResult;
      $reference$4.value = noResult;
      const $action = deliver(fiber)(result);
      const stop = $action();
      const $condition = !stop;
      const $action$1 = runSteps(fiber);
      if ($condition) {
        $action$1();
      }
      return Data_Unit.unit;
    }
    if (status === "Return") {
      const $reference$5 = fiber.status;
      $reference$5.value = "Running";
      const $action$2 = returnStep(fiber);
      const stop$1 = $action$2();
      const $condition$1 = !stop$1;
      const $action$3 = runSteps(fiber);
      if ($condition$1) {
        $action$3();
      }
      return Data_Unit.unit;
    }
    if (status === "Running") {
      return Data_Unit.unit;
    }
    if (status === "Pending") {
      return Data_Unit.unit;
    }
    if (status === "Completed") {
      return Data_Unit.unit;
    }
    throw new Error("Pattern match failure");
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
      return Data_Unit.unit;
    };
  };
}
function runSteps(fiber) {
  const $reference = fiber.current;
  return () => {
    while (true) {
      let $condition;
      const step = $reference.value;
      $case: {
        if (step.tag === "Bind") {
          const { _1: inner, _2: continuation } = step;
          if (inner.tag === "Pure") {
            const { _1: value } = inner;
            $condition = continueWith(fiber)(continuation(value))();
            break $case;
          }
          if (inner.tag === "Sync") {
            const { _1: effect } = inner;
            const value$1 = effect();
            $condition = continueWith(fiber)(continuation(value$1))();
            break $case;
          }
          if (inner.tag === "Fallible") {
            const { _1: effect$1 } = inner;
            const result = effect$1();
            if (result.tag === "Left") {
              const { _1: error } = result;
              $condition = failWith(fiber)(error)();
              break $case;
            }
            if (result.tag === "Right") {
              const { _1: value$2 } = result;
              $condition = continueWith(fiber)(continuation(value$2))();
              break $case;
            }
            throw new Error("Pattern match failure");
          }
          if (inner.tag === "Throw") {
            const { _1: error$1 } = inner;
            $condition = failWith(fiber)(error$1)();
            break $case;
          }
          if (inner.tag === "Run") {
            const { _1: effect$2 } = inner;
            const next = effect$2();
            if (next.tag === "Pure") {
              const { _1: value$3 } = next;
              $condition = continueWith(fiber)(continuation(value$3))();
              break $case;
            }
            if (next.tag === "Sync") {
              const { _1: nextEffect } = next;
              const value$4 = nextEffect();
              $condition = continueWith(fiber)(continuation(value$4))();
              break $case;
            }
            if (next.tag === "Fallible") {
              const { _1: nextEffect$1 } = next;
              const result$1 = nextEffect$1();
              if (result$1.tag === "Left") {
                const { _1: error$2 } = result$1;
                $condition = failWith(fiber)(error$2)();
                break $case;
              }
              if (result$1.tag === "Right") {
                const { _1: value$5 } = result$1;
                $condition = continueWith(fiber)(continuation(value$5))();
                break $case;
              }
              throw new Error("Pattern match failure");
            }
            if (next.tag === "Throw") {
              const { _1: error$3 } = next;
              $condition = failWith(fiber)(error$3)();
              break $case;
            }
            const $action = pushContinuation(fiber)(continuation);
            const $unit = $action();
            $condition = continueWith(fiber)(next)();
            break $case;
          }
          const $action$1 = pushContinuation(fiber)(continuation);
          const $unit$1 = $action$1();
          $condition = continueWith(fiber)(inner)();
          break $case;
        }
        if (step.tag === "Pure") {
          const { _1: value$6 } = step;
          $condition = succeedWith(fiber)(value$6)();
          break $case;
        }
        if (step.tag === "Sync") {
          const { _1: effect$3 } = step;
          const value$7 = effect$3();
          $condition = succeedWith(fiber)(value$7)();
          break $case;
        }
        if (step.tag === "Fallible") {
          const { _1: effect$4 } = step;
          const result$2 = effect$4();
          if (result$2.tag === "Left") {
            const { _1: error$4 } = result$2;
            $condition = failWith(fiber)(error$4)();
            break $case;
          }
          if (result$2.tag === "Right") {
            const { _1: value$8 } = result$2;
            $condition = succeedWith(fiber)(value$8)();
            break $case;
          }
          throw new Error("Pattern match failure");
        }
        if (step.tag === "Throw") {
          const { _1: error$5 } = step;
          $condition = failWith(fiber)(error$5)();
          break $case;
        }
        if (step.tag === "Run") {
          const { _1: effect$5 } = step;
          const next$1 = effect$5();
          $condition = continueWith(fiber)(next$1)();
          break $case;
        }
        if (step.tag === "Async") {
          const { _1: register } = step;
          const $reference$1 = fiber.runTick;
          const localRunTick = $reference$1.value;
          const $reference$2 = fiber.status;
          $reference$2.value = "Pending";
          const $reference$3 = fiber.pendingCanceler;
          $reference$3.value = "Nothing";
          const $scrutinee = fiber.runtime;
          const runtime = $scrutinee;
          const $action$2 = register((result$3) => {
            const $reference$4 = fiber.runTick;
            return () => {
              const tick = $reference$4.value;
              const $condition$1 = tick === localRunTick;
              const $reference$5 = fiber.runTick;
              const $value = tick + (1 | 0) | 0;
              if ($condition$1) {
                $reference$5.value = $value;
                const $function = enqueue(runtime.scheduler);
                const $reference$6 = fiber.runTick;
                $function(() => {
                  const resumeTick = $reference$6.value;
                  const $condition$2 = resumeTick === (localRunTick + (1 | 0) | 0);
                  const $reference$7 = fiber.pendingCanceler;
                  if ($condition$2) {
                    $reference$7.value = "Nothing";
                    const $reference$8 = fiber.runnableResult;
                    $reference$8.value = result$3;
                    const $reference$9 = fiber.status;
                    $reference$9.value = "Runnable";
                    run(fiber)();
                  }
                  return Data_Unit.unit;
                })();
              }
              return Data_Unit.unit;
            };
          });
          const canceler = $action$2();
          const $reference$10 = fiber.status;
          const status = $reference$10.value;
          $case$1: {
            if (status === "Pending") {
              const $reference$11 = fiber.pendingCanceler;
              const pendingCanceler = $reference$11.value;
              const $condition$3 = Data_Maybe.isNothing(pendingCanceler);
              const $reference$12 = fiber.pendingCanceler;
              const $value$1 = {
                tag: "Just",
                _1: canceler
              };
              if ($condition$3) {
                $reference$12.value = $value$1;
              }
              break $case$1;
            }
            break $case$1;
          }
          $condition = true;
          break $case;
        }
        if (step.tag === "Catch") {
          const { _1: inner$1, _2: handler } = step;
          const $reference$13 = fiber.interrupt;
          const interrupt = $reference$13.value;
          const $action$3 = suspendContinuations(fiber);
          const $unit$2 = $action$3();
          const $action$4 = pushAttempt(fiber)({
            tag: "AttemptCatch",
            _1: handler,
            _2: Data_Maybe.isJust(interrupt)
          });
          const $unit$3 = $action$4();
          $condition = continueWith(fiber)(opaqueStep(inner$1))();
          break $case;
        }
        if (step.tag === "Bracket") {
          const { _1: acquire, _2: conditions, _3: use } = step;
          const $reference$14 = fiber.bracketCount;
          const bracketCount = $reference$14.value;
          const $reference$15 = fiber.bracketCount;
          const $value$2 = bracketCount + (1 | 0) | 0;
          $reference$15.value = $value$2;
          const $reference$16 = fiber.interrupt;
          const interrupt$1 = $reference$16.value;
          const $action$5 = suspendContinuations(fiber);
          const $unit$4 = $action$5();
          const $action$6 = pushAttempt(fiber)({
            tag: "AttemptBracket",
            _1: conditions,
            _2: use,
            _3: Data_Maybe.isJust(interrupt$1)
          });
          const $unit$5 = $action$6();
          $condition = continueWith(fiber)(acquire)();
          break $case;
        }
        if (step.tag === "Fork") {
          const { _1: child, _2: continuation$1 } = step;
          const $action$7 = makeFiber(fiber.runtime)(child);
          const forked = $action$7();
          const $scrutinee$1 = fiber.runtime;
          const runtime$1 = $scrutinee$1;
          const $reference$17 = fiber.runnableResult;
          const $value$3 = {
            tag: "Right",
            _1: continuation$1(forked)
          };
          $reference$17.value = $value$3;
          const $reference$18 = fiber.status;
          $reference$18.value = "Runnable";
          const $action$8 = prioritize(runtime$1.scheduler)(run(fiber));
          const $unit$6 = $action$8();
          const $action$9 = prioritize(runtime$1.scheduler)(run(forked));
          const $unit$7 = $action$9();
          const $action$10 = drainUnlessDraining(runtime$1.scheduler);
          const $unit$8 = $action$10();
          $condition = true;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      if ($condition) {
        break;
      }
    }
    return Data_Unit.unit;
  };
}
function returnStep(fiber) {
  const $reference = fiber.continuations;
  return () => {
    $reference.value = "Nil";
    const $reference$1 = fiber.attempts;
    while (true) {
      let $condition;
      const attempts = $reference$1.value;
      $case: {
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
          $case$1: {
            if (defect.tag === "Just") {
              const { _1: error } = defect;
              $result = {
                tag: "Died",
                _1: error
              };
              break $case$1;
            }
            if (interrupt.tag === "Just") {
              const { _1: error$1 } = interrupt;
              $result = {
                tag: "Failed",
                _1: error$1
              };
              break $case$1;
            }
            if (failure.tag === "Just") {
              const { _1: error$2 } = failure;
              $result = {
                tag: "Failed",
                _1: error$2
              };
              break $case$1;
            }
            $result = {
              tag: "Succeeded",
              _1: orUnit(returnValue)
            };
            break $case$1;
          }
          const result = $result;
          const $reference$6 = fiber.exit;
          $reference$6.value = result;
          const $reference$7 = fiber.current;
          $reference$7.value = noStep;
          const $reference$8 = fiber.status;
          $reference$8.value = "Completed";
          $condition = true;
          break $case;
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
                $condition = true;
                break $case;
              }
            }
            $condition = false;
            break $case;
          }
          if (attempt.tag === "AttemptResume") {
            const { _1: continuations, _2: enqueuedInterrupted$3 } = attempt;
            if (killed(enqueuedInterrupted$3) || Data_Maybe.isJust(failure$1) || Data_Maybe.isJust(defect$1)) {
              $condition = false;
              break $case;
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
                $condition = true;
                break $case;
              }
              if (continuations === "Nil") {
                $condition = false;
                break $case;
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
              $condition = false;
              break $case;
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
                $condition = true;
                break $case;
              } else {
                $condition = false;
                break $case;
              }
            }
          }
          if (attempt.tag === "AttemptRelease") {
            const { _1: conditions$1, _2: resource$1, _3: enqueuedInterrupted$5 } = attempt;
            const $reference$22 = fiber.returnValue;
            const returnValue$3 = $reference$22.value;
            let $result$1;
            $case$2: {
              {
                if (killed(enqueuedInterrupted$5)) {
                  $result$1 = conditions$1.killed(resource$1);
                  break $case$2;
                }
              }
              if (defect$1.tag === "Just") {
                const { _1: error$4 } = defect$1;
                $result$1 = conditions$1.died(error$4)(resource$1);
                break $case$2;
              }
              if (failure$1.tag === "Just") {
                const { _1: error$5 } = failure$1;
                $result$1 = conditions$1.failed(error$5)(resource$1);
                break $case$2;
              }
              $result$1 = conditions$1.completed(orUnit(returnValue$3))(resource$1);
              break $case$2;
            }
            const finalizer = $result$1;
            const $action$1 = enterFinalizer(fiber);
            const $unit$1 = $action$1();
            const $reference$23 = fiber.current;
            const $value$4 = opaqueStep(finalizer);
            $reference$23.value = $value$4;
            $condition = true;
            break $case;
          }
          if (attempt.tag === "AttemptFinalizer") {
            const { _1: finalizer$1 } = attempt;
            const $action$2 = enterFinalizer(fiber);
            const $unit$2 = $action$2();
            const $reference$24 = fiber.current;
            $reference$24.value = finalizer$1;
            $condition = true;
            break $case;
          }
          if (attempt.tag === "AttemptFinalized") {
            const { _1: savedValue, _2: savedFailure, _3: savedDefect } = attempt;
            const $reference$25 = fiber.bracketCount;
            const $value$5 = bracketCount - (1 | 0) | 0;
            $reference$25.value = $value$5;
            if (Data_Maybe.isJust(savedDefect)) {
              const $reference$26 = fiber.defect;
              $reference$26.value = savedDefect;
              const $reference$27 = fiber.returnValue;
              $reference$27.value = "Nothing";
              const $reference$28 = fiber.failure;
              $reference$28.value = "Nothing";
            } else {
              if (Data_Maybe.isJust(defect$1)) {
                const $reference$29 = fiber.returnValue;
                $reference$29.value = "Nothing";
                const $reference$30 = fiber.failure;
                $reference$30.value = "Nothing";
              } else {
                if (Data_Maybe.isNothing(savedFailure) && Data_Maybe.isJust(failure$1)) {
                  const $reference$31 = fiber.returnValue;
                  $reference$31.value = "Nothing";
                } else {
                  const $reference$32 = fiber.returnValue;
                  $reference$32.value = savedValue;
                  const $reference$33 = fiber.failure;
                  $reference$33.value = savedFailure;
                }
              }
            }
            $condition = false;
            break $case;
          }
          throw new Error("Pattern match failure");
        }
        throw new Error("Pattern match failure");
      }
      if ($condition) {
        break;
      }
    }
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
      while (true) {
        let $condition;
        const pending = remaining;
        $case: {
          if (pending.tag === "Cons") {
            const { _1: entry, _2: rest } = pending;
            remaining = rest;
            const $action = tryEffect(entry.join(exit));
            const outcome = $action();
            const $action$1 = recordFirstEscape(escaped)(outcome);
            const $unit = $action$1();
            $condition = false;
            break $case;
          }
          if (pending === "Nil") {
            $condition = true;
            break $case;
          }
          throw new Error("Pattern match failure");
        }
        if ($condition) {
          break;
        }
      }
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

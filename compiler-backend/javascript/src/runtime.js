export function binding(name, initialize) {
  let state = 0;
  let value;
  return () => {
    if (state === 2) return value;
    if (state === 1) {
      throw new ReferenceError(`${name} was needed before it finished initializing`);
    }
    state = 1;
    value = initialize();
    state = 2;
    return value;
  };
}

export const syncPure = (value) => () => value;

export const syncBind = (action) => (continuation) => () =>
  continuation(action())();

export const syncDiscard = syncBind;

export const syncMap = (function_) => (action) => () => function_(action());

export const syncApply = (functionAction) => (argumentAction) => () =>
  functionAction()(argumentAction());

class IrisAbort {
  constructor(identity, error) {
    this.identity = identity;
    this.error = error;
  }
}

export const syncAbort = (identity) => (error) => () => {
  throw new IrisAbort(identity, error);
};

export const syncCatchAbort = (identity) => (program) => (handler) => () => {
  try {
    return program();
  } catch (error) {
    if (error instanceof IrisAbort && error.identity === identity) {
      return handler(error.error)();
    }
    throw error;
  }
};

const PURE = 0;
const BIND = 1;
const LIFT = 2;
const DEFER = 3;
const YIELD = 4;
const REGISTER = 5;
const BRACKET = 6;
const JOIN = 7;
const INTERRUPT = 8;
const ABORT = 9;
const CATCH_ABORT = 10;

const FRAME_BIND = 0;
const FRAME_ACQUIRE = 1;
const FRAME_FINALIZER = 2;
const FRAME_RESTORE = 3;
const FRAME_CATCH_ABORT = 4;

const STATUS_RUNNING = 0;
const STATUS_SUSPENDED = 1;
const STATUS_COMPLETED = 2;

const OUTCOME_SUCCESS = 0;
const OUTCOME_DEFECT = 1;
const OUTCOME_INTERRUPTED = 2;
const OUTCOME_ABORT = 3;

// Bound each drain so a recursively generated computation cannot starve the host event loop.
const OPERATIONS_PER_HOST_TURN = 2048;
const UNIT = Object.freeze({});

function successOutcome(value) {
  return { tag: OUTCOME_SUCCESS, value };
}

function defectOutcome(error) {
  return { tag: OUTCOME_DEFECT, error };
}

function abortOutcome(identity, error) {
  return { tag: OUTCOME_ABORT, identity, error };
}

const interruptionOutcome = Object.freeze({ tag: OUTCOME_INTERRUPTED });

export const asyncPure = (value) => ({ tag: PURE, value });

export const asyncBind = (action) => (continuation) =>
  ({ tag: BIND, action, continuation });

export const asyncDiscard = asyncBind;

export const asyncMap = (function_) => (action) =>
  ({ tag: BIND, action, continuation: (value) => asyncPure(function_(value)) });

export const asyncApply = (functionAction) => (argumentAction) =>
  ({
    tag: BIND,
    action: functionAction,
    continuation: (function_) => ({
      tag: BIND,
      action: argumentAction,
      continuation: (argument) => asyncPure(function_(argument)),
    }),
  });

export const asyncLift = (action) => ({ tag: LIFT, action });

export const asyncDefer = (factory) => ({ tag: DEFER, factory });

export const asyncYield = Object.freeze({ tag: YIELD });

export const asyncRegister = (registration) => ({ tag: REGISTER, registration });

export const asyncBracket = (acquire) => (release) => (use) =>
  ({ tag: BRACKET, acquire, release, use });

export const asyncJoin = (fiber) => ({ tag: JOIN, fiber });

export const asyncInterrupt = (fiber) => ({ tag: INTERRUPT, fiber });

export const asyncAbort = (identity) => (error) => ({ tag: ABORT, identity, error });

export const asyncCatchAbort = (identity) => (program) => (handler) =>
  ({ tag: CATCH_ABORT, identity, program, handler });

let readyFibers = [];
let readyFiberIndex = 0;
let deferredFibers = [];
let isDraining = false;
let isHostDrainScheduled = false;

function scheduleHostDrain() {
  if (isHostDrainScheduled) return;
  isHostDrainScheduled = true;
  const schedule = typeof setImmediate === "function" ? setImmediate : setTimeout;
  schedule(() => {
    isHostDrainScheduled = false;
    for (const fiber of deferredFibers) readyFibers.push(fiber);
    deferredFibers = [];
    drainReadyFibers();
  }, 0);
}

function enqueueFiber(fiber, deferToHost) {
  if (fiber.isQueued || fiber.status === STATUS_COMPLETED) return;
  fiber.isQueued = true;
  if (deferToHost) {
    deferredFibers.push(fiber);
    scheduleHostDrain();
  } else {
    readyFibers.push(fiber);
    if (!isDraining) drainReadyFibers();
  }
}

function drainReadyFibers() {
  if (isDraining) return;
  isDraining = true;
  let operationsExecuted = 0;
  try {
    while (readyFiberIndex < readyFibers.length) {
      const fiber = readyFibers[readyFiberIndex++];
      fiber.isQueued = false;
      operationsExecuted += runFiber(fiber);
      if (
        operationsExecuted >= OPERATIONS_PER_HOST_TURN &&
        readyFiberIndex < readyFibers.length
      ) {
        readyFibers = readyFibers.slice(readyFiberIndex);
        readyFiberIndex = 0;
        scheduleHostDrain();
        break;
      }
    }
  } finally {
    if (readyFiberIndex === readyFibers.length) {
      readyFibers = [];
      readyFiberIndex = 0;
    }
    isDraining = false;
  }
}

class Fiber {
  constructor(program) {
    this.instruction = program;
    this.stack = [];
    this.outcome = undefined;
    this.status = STATUS_RUNNING;
    this.isQueued = false;
    this.interruptionMaskDepth = 0;
    this.isInterruptionPending = false;
    this.callbackGeneration = 0;
    this.cancellationAction = undefined;
    this.observers = [];
    this.promise = new Promise((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
    });
    this.promise.catch(() => {});
  }

  observe(observer) {
    if (this.status === STATUS_COMPLETED) {
      observer(this.outcome);
      return () => {};
    }
    this.observers.push(observer);
    return () => {
      const index = this.observers.indexOf(observer);
      if (index >= 0) this.observers.splice(index, 1);
    };
  }

  interrupt() {
    if (this.status === STATUS_COMPLETED || this.isInterruptionPending) return;
    this.isInterruptionPending = true;
    this.beginPendingInterruption();
  }

  beginPendingInterruption() {
    if (this.status !== STATUS_SUSPENDED || this.interruptionMaskDepth !== 0) return;
    this.status = STATUS_RUNNING;
    this.callbackGeneration += 1;
    const cancellationAction = this.cancellationAction;
    this.cancellationAction = undefined;
    if (cancellationAction === undefined) {
      this.outcome = interruptionOutcome;
    } else {
      this.interruptionMaskDepth += 1;
      this.stack.push({ tag: FRAME_RESTORE, outcome: interruptionOutcome });
      this.instruction = cancellationAction;
    }
    enqueueFiber(this, false);
  }
}

function completeFiber(fiber) {
  fiber.status = STATUS_COMPLETED;
  const outcome = fiber.outcome;
  if (outcome.tag === OUTCOME_SUCCESS) {
    fiber.resolve(outcome.value);
  } else if (outcome.tag === OUTCOME_DEFECT) {
    fiber.reject(outcome.error);
  } else if (outcome.tag === OUTCOME_ABORT) {
    fiber.reject(new IrisAbort(outcome.identity, outcome.error));
  } else {
    fiber.reject(new Error("Async computation interrupted"));
  }
  const observers = fiber.observers;
  fiber.observers = [];
  for (const observer of observers) observer(outcome);
}

function continueOutcome(fiber) {
  while (fiber.stack.length > 0) {
    const frame = fiber.stack.pop();
    if (frame.tag === FRAME_BIND) {
      if (fiber.outcome.tag !== OUTCOME_SUCCESS) return true;
      try {
        fiber.instruction = frame.continuation(fiber.outcome.value);
        fiber.outcome = undefined;
      } catch (error) {
        fiber.outcome = defectOutcome(error);
      }
      return true;
    }
    if (frame.tag === FRAME_CATCH_ABORT) {
      if (
        fiber.outcome.tag !== OUTCOME_ABORT ||
        fiber.outcome.identity !== frame.identity
      ) {
        return true;
      }
      try {
        fiber.instruction = frame.handler(fiber.outcome.error);
        fiber.outcome = undefined;
      } catch (error) {
        fiber.outcome = defectOutcome(error);
      }
      return true;
    }
    if (frame.tag === FRAME_ACQUIRE) {
      fiber.interruptionMaskDepth -= 1;
      if (fiber.outcome.tag !== OUTCOME_SUCCESS) return true;
      const resource = fiber.outcome.value;
      fiber.stack.push({ tag: FRAME_FINALIZER, release: frame.release, resource });
      if (fiber.isInterruptionPending && fiber.interruptionMaskDepth === 0) {
        fiber.outcome = interruptionOutcome;
        return true;
      }
      try {
        fiber.instruction = frame.use(resource);
        fiber.outcome = undefined;
      } catch (error) {
        fiber.outcome = defectOutcome(error);
      }
      return true;
    }
    if (frame.tag === FRAME_FINALIZER) {
      const originalOutcome = fiber.outcome;
      fiber.interruptionMaskDepth += 1;
      fiber.stack.push({ tag: FRAME_RESTORE, outcome: originalOutcome });
      try {
        fiber.instruction = frame.release(frame.resource);
        fiber.outcome = undefined;
      } catch (error) {
        fiber.outcome = defectOutcome(error);
      }
      return true;
    }
    if (frame.tag === FRAME_RESTORE) {
      fiber.interruptionMaskDepth -= 1;
      if (fiber.outcome.tag === OUTCOME_SUCCESS) {
        fiber.outcome = fiber.isInterruptionPending && fiber.interruptionMaskDepth === 0
          ? interruptionOutcome
          : frame.outcome;
      } else {
        fiber.outcome = defectOutcome({
          message: "Async cleanup failed",
          outcome: frame.outcome,
          cleanup: fiber.outcome,
        });
      }
      return true;
    }
  }
  completeFiber(fiber);
  return false;
}

function suspendOnFiber(fiber, targetFiber, interruptsTarget) {
  fiber.status = STATUS_SUSPENDED;
  const callbackGeneration = ++fiber.callbackGeneration;
  let removeObserver = () => {};
  const resume = (outcome) => {
    if (
      fiber.status !== STATUS_SUSPENDED ||
      fiber.callbackGeneration !== callbackGeneration
    ) return;
    fiber.status = STATUS_RUNNING;
    fiber.cancellationAction = undefined;
    if (interruptsTarget) {
      fiber.instruction = asyncPure(UNIT);
    } else if (outcome.tag === OUTCOME_SUCCESS) {
      fiber.instruction = asyncPure(outcome.value);
    } else {
      fiber.outcome = outcome;
    }
    enqueueFiber(fiber, false);
  };
  removeObserver = targetFiber.observe(resume);
  fiber.cancellationAction = asyncLift(() => {
    removeObserver();
    return UNIT;
  });
  if (interruptsTarget) targetFiber.interrupt();
}

function registerAsync(fiber, registration) {
  const callbackGeneration = ++fiber.callbackGeneration;
  let registrationInProgress = true;
  let resumedProgram;
  const resume = (program) => () => {
    if (fiber.callbackGeneration !== callbackGeneration || resumedProgram !== undefined) {
      return UNIT;
    }
    resumedProgram = program;
    if (!registrationInProgress && fiber.status === STATUS_SUSPENDED) {
      fiber.status = STATUS_RUNNING;
      fiber.cancellationAction = undefined;
      fiber.instruction = resumedProgram;
      enqueueFiber(fiber, false);
    }
    return UNIT;
  };
  let cancellationAction;
  try {
    cancellationAction = registration(resume)();
  } catch (error) {
    registrationInProgress = false;
    fiber.outcome = defectOutcome(error);
    return true;
  }
  registrationInProgress = false;
  if (resumedProgram !== undefined) {
    fiber.instruction = resumedProgram;
    return true;
  }
  fiber.status = STATUS_SUSPENDED;
  fiber.cancellationAction = cancellationAction;
  if (fiber.isInterruptionPending) fiber.beginPendingInterruption();
  return false;
}

function runFiber(fiber) {
  let operationsExecuted = 0;
  while (fiber.status === STATUS_RUNNING) {
    if (++operationsExecuted >= OPERATIONS_PER_HOST_TURN) {
      enqueueFiber(fiber, true);
      return operationsExecuted;
    }
    if (fiber.outcome !== undefined) {
      if (!continueOutcome(fiber)) return operationsExecuted;
      continue;
    }
    if (fiber.isInterruptionPending && fiber.interruptionMaskDepth === 0) {
      fiber.outcome = interruptionOutcome;
      continue;
    }
    const instruction = fiber.instruction;
    try {
      switch (instruction.tag) {
        case PURE:
          fiber.outcome = successOutcome(instruction.value);
          break;
        case BIND:
          fiber.stack.push({ tag: FRAME_BIND, continuation: instruction.continuation });
          fiber.instruction = instruction.action;
          break;
        case LIFT:
          try {
            fiber.outcome = successOutcome(instruction.action());
          } catch (error) {
            fiber.outcome = error instanceof IrisAbort
              ? abortOutcome(error.identity, error.error)
              : defectOutcome(error);
          }
          break;
        case DEFER:
          fiber.instruction = instruction.factory(UNIT);
          break;
        case YIELD:
          fiber.instruction = asyncPure(UNIT);
          enqueueFiber(fiber, true);
          return operationsExecuted;
        case REGISTER:
          if (!registerAsync(fiber, instruction.registration)) return operationsExecuted;
          break;
        case BRACKET:
          fiber.interruptionMaskDepth += 1;
          fiber.stack.push({
            tag: FRAME_ACQUIRE,
            release: instruction.release,
            use: instruction.use,
          });
          fiber.instruction = instruction.acquire;
          break;
        case JOIN:
          suspendOnFiber(fiber, instruction.fiber, false);
          return operationsExecuted;
        case INTERRUPT:
          suspendOnFiber(fiber, instruction.fiber, true);
          return operationsExecuted;
        case ABORT:
          fiber.outcome = abortOutcome(instruction.identity, instruction.error);
          break;
        case CATCH_ABORT:
          fiber.stack.push({
            tag: FRAME_CATCH_ABORT,
            identity: instruction.identity,
            handler: instruction.handler,
          });
          fiber.instruction = instruction.program;
          break;
        default:
          fiber.outcome = defectOutcome(new Error("Unknown Async instruction"));
      }
    } catch (error) {
      fiber.outcome = defectOutcome(error);
    }
  }
  return operationsExecuted;
}

export const asyncRun = (program) => () => {
  const fiber = new Fiber(program);
  enqueueFiber(fiber, false);
  return fiber;
};

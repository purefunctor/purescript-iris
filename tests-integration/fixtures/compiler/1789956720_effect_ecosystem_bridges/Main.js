let effectsExecuted = 0;
let rejectPending;

export const countedEffect = (value) => () => {
  effectsExecuted += 1;
  return value;
};

export const delayedPromise = (value) => () =>
  new Promise((resolve) => setTimeout(() => resolve(value), 0));

export const rejectedPromise = () => Promise.reject(new Error("promise rejected"));

export const throwingPromise = () => {
  throw new Error("promise factory threw");
};

export const pendingPromise = () => new Promise((_resolve, reject) => {
  rejectPending = reject;
});

export const effectExecutions = () => effectsExecuted;

export const rejectPendingPromise = () => rejectPending(new Error("late rejection"));

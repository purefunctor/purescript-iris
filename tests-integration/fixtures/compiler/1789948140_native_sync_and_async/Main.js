import * as runtime from "../runtime.js";

export const events = [];

export const mark = (label) => (value) => () => {
  events.push(label);
  return value;
};

export const delayed = (value) => runtime.asyncRegister((resume) => () => {
  const timeout = setTimeout(() => resume(runtime.asyncPure(value))(), 0);
  return runtime.asyncLift(() => {
    clearTimeout(timeout);
    return {};
  });
});

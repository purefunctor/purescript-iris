const isError = (thrown) => {
  try {
    return thrown instanceof Error || Object.prototype.toString.call(thrown) === "[object Error]";
  } catch {
    return false;
  }
};

const describeThrown = (thrown) => {
  try {
    return String(thrown);
  } catch {
    return "non-error value";
  }
};

const hostDefect = (thrown) => (isError(thrown) ? thrown : new Error(describeThrown(thrown), { cause: thrown }));

export const tryHost = (onDefect, onSuccess, effect) => {
  try {
    effect();
    return onSuccess;
  } catch (thrown) {
    return onDefect(hostDefect(thrown));
  }
};

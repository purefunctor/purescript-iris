export const boom = (message) => () => {
  throw new Error(message);
};

export const throwNull = () => {
  throw null;
};

export const throwHostile = () => {
  throw {
    get [Symbol.toStringTag]() {
      throw new Error("classification defect");
    },
    toString() {
      throw new Error("description defect");
    },
  };
};

export const setTimeout = (milliseconds) => (effect) => () => globalThis.setTimeout(effect, milliseconds);

export const clearTimeout = (identifier) => () => globalThis.clearTimeout(identifier);

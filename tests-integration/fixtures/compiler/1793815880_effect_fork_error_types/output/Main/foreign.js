export const setTimeout = (milliseconds) => (effect) => () => globalThis.setTimeout(effect, milliseconds);

export const clearTimeout = (identifier) => () => globalThis.clearTimeout(identifier);

export const messages = [];

export const logEffect = (message) => () => {
  messages.push(message);
};

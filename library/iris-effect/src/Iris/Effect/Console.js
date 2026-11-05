export const logToConsole = (message) => () => {
  console.log(message);
};

export const errorToConsole = (message) => () => {
  console.error(message);
};

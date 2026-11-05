import * as $foreign from "./foreign.js";
export function log(services) {
  return (message) => {
    return (environment) => {
      const $value = services["Iris.Effect.Console.Console"](environment);
      return () => {
        const syncArgument = $value.log(message);
        return syncArgument();
      };
    };
  };
}
export function error(services) {
  return (message) => {
    return (environment) => {
      const $value = services["Iris.Effect.Console.Console"](environment);
      return () => {
        const syncArgument = $value.error(message);
        return syncArgument();
      };
    };
  };
}
const logToConsole = $foreign["logToConsole"];
const errorToConsole = $foreign["errorToConsole"];
const $default = {
  log: logToConsole,
  error: errorToConsole
};
export const defaultServiceConsole = { defaultService: $default };
export { $default as "default" };

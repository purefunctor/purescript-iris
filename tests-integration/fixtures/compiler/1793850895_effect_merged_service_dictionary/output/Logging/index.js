export function logLine(services) {
  return (message) => {
    return (environment) => {
      const $value = services["Logging.Logger"](environment);
      return () => {
        const syncArgument = $value(message);
        return syncArgument();
      };
    };
  };
}

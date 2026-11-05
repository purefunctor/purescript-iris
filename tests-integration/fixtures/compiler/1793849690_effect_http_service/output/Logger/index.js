export function info(services) {
  return (message) => {
    return (environment) => {
      const $value = services["Logger.Logger"](environment);
      return () => {
        const syncArgument = $value.write(message);
        return syncArgument();
      };
    };
  };
}

export function addToReference(reference, amount) {
  const section56 = reference.value;
  const $value = section56 + amount | 0;
  return reference.value = $value;
}
export function addFive($function) {
  return (reference) => {
    return () => {
      return $function(reference, 5 | 0);
    };
  };
}
export function logTwice(log) {
  return (message) => {
    return () => {
      const $unit = log(message);
      const $argument = message + "!";
      return log($argument);
    };
  };
}

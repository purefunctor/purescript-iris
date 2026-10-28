export function describe(source) {
  return (log) => {
    return () => {
      const value = source.value;
      if (value === (0 | 0)) {
        log.value = [...log.value, "zero"];
        return;
      }
      if (value === (1 | 0)) {
        log.value = [...log.value, "one"];
        return;
      }
      log.value = [...log.value, "many"];
    };
  };
}
export function describeGuarded(source) {
  return (log) => {
    return () => {
      const value = source.value;
      {
        const n = value;
        if (n > (10 | 0)) {
          log.value = [...log.value, "big"];
          return;
        }
        if (n > (0 | 0)) {
          log.value = [...log.value, "small"];
          return;
        }
      }
      log.value = [...log.value, "none"];
    };
  };
}
export function describeEach(values) {
  const $value = [];
  return () => {
    const log = { value: $value };
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      if (value === (0 | 0)) {
        log.value = [...log.value, "zero"];
        continue;
      }
      if (value === (1 | 0)) {
        log.value = [...log.value, "one"];
        continue;
      }
      log.value = [...log.value, "many"];
    }
    return log.value;
  };
}
export function describeCountdown(start) {
  return () => {
    const counter = { value: start };
    const $value = [];
    const log = { value: $value };
    const $function = (section349) => section349 >= (0 | 0);
    while (true) {
      let $condition;
      $condition = $function(counter.value);
      if (!$condition) {
        break;
      }
      const current = counter.value;
      const $value$1 = current - (1 | 0) | 0;
      counter.value = $value$1;
      if (current === (0 | 0)) {
        log.value = [...log.value, "zero"];
        continue;
      }
      if (current === (1 | 0)) {
        log.value = [...log.value, "one"];
        continue;
      }
      log.value = [...log.value, "many"];
    }
    return log.value;
  };
}

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
    let log = $value;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      if (value === (0 | 0)) {
        log = [...log, "zero"];
        continue;
      }
      if (value === (1 | 0)) {
        log = [...log, "one"];
        continue;
      }
      log = [...log, "many"];
    }
    return log;
  };
}
export function describeCountdown(start) {
  return () => {
    let counter = start;
    const $value = [];
    let log = $value;
    const $function = (section349) => section349 >= (0 | 0);
    while (true) {
      let $condition;
      $condition = $function(counter);
      if (!$condition) {
        break;
      }
      const current = counter;
      const $value$1 = current - (1 | 0) | 0;
      counter = $value$1;
      if (current === (0 | 0)) {
        log = [...log, "zero"];
        continue;
      }
      if (current === (1 | 0)) {
        log = [...log, "one"];
        continue;
      }
      log = [...log, "many"];
    }
    return log;
  };
}

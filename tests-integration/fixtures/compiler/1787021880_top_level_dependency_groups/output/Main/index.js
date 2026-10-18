function $tail_first_second($state, $argument0) {
  while (true) {
    switch ($state) {
      // first
      case 0: {
        const $currentArgument0 = $argument0;
        if ($currentArgument0 === true) {
          return 1 | 0;
        }
        if ($currentArgument0 === false) {
          $argument0 = true;
          $state = 1;
          continue;
        }
        throw new Error("Pattern match failure");
      }
      // second
      case 1: {
        const $currentArgument0$1 = $argument0;
        if ($currentArgument0$1 === true) {
          return 2 | 0;
        }
        if ($currentArgument0$1 === false) {
          $argument0 = true;
          $state = 0;
          continue;
        }
        throw new Error("Pattern match failure");
      }
    }
  }
}
export function first($boolean) {
  return $tail_first_second(0, $boolean);
}
export function second($boolean$1) {
  return $tail_first_second(1, $boolean$1);
}
export const later = 42 | 0;
export const forward = later;
export const recursiveValue = { run: (value) => value ? 3 | 0 : recursivePeer.run(true) };
export const recursivePeer = { run: (value) => value ? 4 | 0 : recursiveValue.run(true) };

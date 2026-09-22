## Ownership

The backend turns checked frontend representations into executable JavaScript and validates foreign
JavaScript modules. Preserve the direction `frontend → functional → javascript`. Backend
representations should be owned; frontend crates must not depend on code generation.

## Output contracts

- Generated modules target ES2022 and Node.js 22 or newer. This is an output compatibility contract,
  separate from the Node.js version required to run the integration-test harness.
- Report unsupported frontend states explicitly rather than silently emitting incorrect JavaScript.
- Keep pretty-printers useful for diagnosing intermediate functional trees.
- Run `just t compiler` whenever generated JavaScript, foreign-module validation, or executable
  behavior can change. Update fixture output through `just t compiler <filters> --update-output`,
  then inspect every changed file.

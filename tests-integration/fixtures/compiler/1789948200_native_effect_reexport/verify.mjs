import { strictEqual } from "node:assert";

import * as Main from "./output/Main/index.js";

strictEqual(Main.pure(42)(), 42);

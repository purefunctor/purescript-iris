import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

strictEqual(Main.fs({})[0](Main.Chosen(17)), 17);

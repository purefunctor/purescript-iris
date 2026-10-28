import { strictEqual } from "node:assert/strict";
import * as STRef from "./output/Control.Monad.ST.Internal/index.js";
import * as Main from "./output/Main/index.js";

const reference = STRef.new(1)();
strictEqual(Main.addToReference.length, 2);
strictEqual(Main.addToReference(reference, 2), 3);
strictEqual(STRef.read(reference)(), 3);
strictEqual(Main.addTwice(10), 15);

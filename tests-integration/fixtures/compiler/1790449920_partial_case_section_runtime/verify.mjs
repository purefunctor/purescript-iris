import { strictEqual, throws } from "node:assert/strict";
import * as Main from "./output/Main/index.js";

let reads = 0;
const effect = Main.test(() => {
  reads += 1;
  return "blob";
});
strictEqual(reads, 0);
strictEqual(effect(), Main.Blob);
strictEqual(reads, 1);
strictEqual(Main.test(() => "arraybuffer")(), Main.ArrayBuffer);
throws(() => Main.test(() => "invalid")(), { message: "Pattern match failure" });
strictEqual(Main.test3("blob"), Main.Blob);
strictEqual(Main.test3("arraybuffer"), Main.ArrayBuffer);
strictEqual(Main.test4(Main.Blob), 17);
strictEqual(Main.test5(() => Main.Blob)(), 17);
strictEqual(Main.test6(Main.Blob), 23);
throws(() => Main.test6(Main.ArrayBuffer), { message: "Pattern match failure" });

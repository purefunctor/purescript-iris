import assert from 'node:assert/strict';
import * as Main from './output/Main/index.js';
assert.equal(Main.mapped, 13);
assert.equal(Main.folded, 17);
assert.equal(Main.traversed, 13);
assert.equal(Main.contramapped, 13);
assert.equal(Main.equal, false);
assert.equal(Main.ordered, true);

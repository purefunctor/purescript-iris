import assert from 'node:assert/strict';
import * as Main from './output/Main/index.js';

assert.equal(Main.mapped, true);
assert.equal(Main.folded, '3right');
assert.equal(Main.traversed, true);

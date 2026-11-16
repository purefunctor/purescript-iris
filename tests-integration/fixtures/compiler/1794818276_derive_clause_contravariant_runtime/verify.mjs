import assert from 'node:assert/strict';
import * as Main from './output/Main/index.js';

assert.equal(Main.predicate, true);
assert.equal(Main.transformed, '11!');

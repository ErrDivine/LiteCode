import assert from 'node:assert/strict';
import { applyDiscount } from '../src/discountRules.js';

assert.equal(applyDiscount(100, { tier: 'vip' }), 90);
assert.equal(applyDiscount(8, { coupon: 'SHIPFREE' }), 3);
assert.equal(applyDiscount(4, { coupon: 'SHIPFREE' }), 0);
console.log('discount rules test passed');

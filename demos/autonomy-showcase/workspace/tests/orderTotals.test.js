import assert from 'node:assert/strict';
import { invoiceTotal } from '../src/orderTotals.js';

const items = [
  { sku: 'MONITOR', qty: 2, price: 100, taxable: true },
  { sku: 'CABLE', qty: 1, price: 20, taxable: false }
];

assert.equal(invoiceTotal(items, { taxRate: 0.1, shipping: 7 }), 247);
console.log('order total test passed');

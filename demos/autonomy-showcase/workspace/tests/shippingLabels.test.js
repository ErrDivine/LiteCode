import assert from 'node:assert/strict';
import { shipmentLabel } from '../src/shippingLabels.js';

assert.equal(
  shipmentLabel(
    { id: 'SO-1042', priority: 'express' },
    { city: 'Seattle', region: 'WA' }
  ),
  'EXPRESS / SO-1042 / Seattle / WA'
);
console.log('shipping label test passed');

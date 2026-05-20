import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const demoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const workspace = path.join(demoRoot, 'workspace');

const files = {
  'src/orderTotals.js': `// @ts-check

/**
 * @typedef {Object} LineItem
 * @property {string} sku
 * @property {number} qty
 * @property {number} price
 * @property {boolean=} taxable
 */

/**
 * @param {LineItem[]} items
 */
export function subtotal(items) {
  return items.reduce((sum, item) => sum + item.qty * item.price, 0);
}

/**
 * @param {LineItem[]} items
 * @param {number} rate
 */
export function tax(items, rate) {
  const taxableSubtotal = items
    .filter((item) => item.taxable !== false)
    .reduce((sum, item) => sum + item.qty * item.price, 0);
  return taxableSubtotal * rate;
}

/**
 * @param {LineItem[]} items
 * @param {{ taxRate?: number, shipping?: number }} options
 */
export function invoiceTotal(items, options = {}) {
  const taxRate = options.taxRate ?? 0;
  const shipping = options.shipping ?? 0;
  return subtotal(items) + tax(items, taxRate) + shipping;
}
`,
  'src/discountRules.js': `// @ts-check

/**
 * @param {number} total
 * @param {{ tier?: string, coupon?: string }} customer
 */
export function applyDiscount(total, customer = {}) {
  if (customer.coupon === 'SHIPFREE') {
    return Math.max(total - 5, 0);
  }
  if (customer.tier === 'vip') {
    return total * 0.9;
  }
  return total;
}
`,
  'src/shippingLabels.js': `// @ts-check

/**
 * @param {{ id: string, priority?: 'standard' | 'express' }} order
 * @param {{ city: string, region: string }} destination
 */
export function shipmentLabel(order, destination) {
  const priority = order.priority === 'express' ? 'EXPRESS' : 'STANDARD';
  return [priority, order.id, destination.city, destination.region].join(' / ');
}
`,
  'src/dashboard.html': `<main class="dashboard" aria-label="Operations dashboard">
  <section class="metric" aria-label="Pending orders">
    <h2>Pending orders</h2>
    <strong>24</strong>
  </section>
  <section class="metric" aria-label="Risk holds">
    <h2>Risk holds</h2>
    <strong>3</strong>
  </section>
</main>
`,
  'tests/orderTotals.test.js': `import assert from 'node:assert/strict';
import { invoiceTotal } from '../src/orderTotals.js';

const items = [
  { sku: 'MONITOR', qty: 2, price: 100, taxable: true },
  { sku: 'CABLE', qty: 1, price: 20, taxable: false }
];

assert.equal(invoiceTotal(items, { taxRate: 0.1, shipping: 7 }), 247);
console.log('order total test passed');
`,
  'tests/discountRules.test.js': `import assert from 'node:assert/strict';
import { applyDiscount } from '../src/discountRules.js';

assert.equal(applyDiscount(100, { tier: 'vip' }), 90);
assert.equal(applyDiscount(8, { coupon: 'SHIPFREE' }), 3);
assert.equal(applyDiscount(4, { coupon: 'SHIPFREE' }), 0);
console.log('discount rules test passed');
`,
  'tests/shippingLabels.test.js': `import assert from 'node:assert/strict';
import { shipmentLabel } from '../src/shippingLabels.js';

assert.equal(
  shipmentLabel(
    { id: 'SO-1042', priority: 'express' },
    { city: 'Seattle', region: 'WA' }
  ),
  'EXPRESS / SO-1042 / Seattle / WA'
);
console.log('shipping label test passed');
`,
  'scripts/checkReleaseNotes.js': `import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const text = readFileSync(new URL('../docs/release-notes.md', import.meta.url), 'utf8');
for (const heading of ['Autonomous Suggestions', 'Skill Routing', 'Rollback Safety']) {
  assert.match(text, new RegExp('## ' + heading), 'missing release-note section: ' + heading);
}
console.log('release notes check passed');
`,
  'scripts/checkDashboardA11y.js': `import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../src/dashboard.html', import.meta.url), 'utf8');
assert.match(html, /<main\\b[^>]*aria-label="Operations dashboard"/, 'dashboard needs a labelled main landmark');
assert.match(html, /<section\\b[^>]*aria-label="Pending orders"/, 'pending orders metric needs an accessible label');
assert.match(html, /<section\\b[^>]*aria-label="Risk holds"/, 'risk holds metric needs an accessible label');
console.log('dashboard accessibility check passed');
`,
  'docs/release-notes.md': `# Release Notes

## Autonomous Suggestions

- Marvis observes editor status and proposes focused agent work after user approval.

## Skill Routing

- Skills and toolsets are discovered from the runtime registry and used as agent identities.

## Rollback Safety

- Workspace writes are protected by preimage snapshots before patch tools run.
`
};

for (const relativeDir of ['src', 'tests', 'scripts', 'docs']) {
  mkdirSync(path.join(workspace, relativeDir), { recursive: true });
}

rmSync(path.join(workspace, 'src', 'fulfillment'), { recursive: true, force: true });

for (const [relativePath, contents] of Object.entries(files)) {
  const target = path.join(workspace, relativePath);
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(target, contents, 'utf8');
}

console.log('Marvis autonomy showcase workspace reset to baseline');

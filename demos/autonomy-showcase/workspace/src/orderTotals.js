// @ts-check

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
  return Math.round(taxableSubtotal * rate * 100) / 100;
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

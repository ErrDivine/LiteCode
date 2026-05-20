// @ts-check

/**
 * @param {number} total
 * @param {{ tier?: string, coupon?: string }} customer
 */
export function applyDiscount(total, customer = {}) {
  if (customer.coupon === 'SHIPFREE') {
    return Math.max(total - 5, 0);
  }
  if (customer.tier === 'vip') {
    return Math.round(total * 0.9 * 100) / 100;
  }
  return total;
}

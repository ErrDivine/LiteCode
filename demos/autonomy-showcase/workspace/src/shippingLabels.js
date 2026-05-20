// @ts-check

/**
 * @param {{ id: string, priority?: 'standard' | 'express' }} order
 * @param {{ city: string, region: string }} destination
 */
export function shipmentLabel(order, destination) {
  const priority = order.priority === 'express' ? 'EXPRESS' : 'STANDARD';
  return [priority, order.id, destination.city, destination.region].join(' / ');
}

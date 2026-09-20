import primary, { value as importedValue } from './cycle-a.js';
export { next as cycleNext } from './cycle-a.js';
export * from './cycle-b.js';

const legacy = require('./legacy.cjs');
const deferred = require(moduleName);

module.exports = { primary };
exports.named = importedValue;

export async function optional(loader, object, key) {
  const shadow = () => importedValue;
  await loader?.load?.(shadow());
  return object[key]();
}

export const created = new Widget();
export const later = import('./lazy.js');
export const unknownLater = import(moduleName);
export const text = 'phantom_call()';

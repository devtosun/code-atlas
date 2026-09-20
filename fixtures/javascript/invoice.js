import { double as twice } from './math.js';
export class Invoice {
  constructor(amount) { this.amount = amount; }
  total() { return twice(this.amount); }
}
export function shadow(twice) { return twice(2); }
export async function load(loader) { return await loader(); }
export const text = 'phantom_call()';

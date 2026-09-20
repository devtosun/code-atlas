import type { Invoice as InvoiceShape } from './models';
import { total as runtimeTotal } from './models';
export type { InvoiceId as ExternalId } from './models';
export { runtimeTotal as calculate };

export interface Token<T> {
  map<U>(value: U): T;
}

export const Token = <T>(value: T): Token<T> => ({
  map: () => value,
});

export type Mapper<T, U> = (value: T) => U;
export enum Mode { Draft, Ready }
export namespace Billing { export const version = 1; }

export function parse(value: string): string;
export function parse(value: number): number;
export function parse(value: string | number): string | number { return value; }

@sealed
export class Repository<T> {
  @logged
  async save<U>(value: U): Promise<U> { return value; }
}

export function computed(object: Record<string, () => number>, key: string): number {
  return object[key]();
}

export const metin = 'yorum_çağrısı()';

export interface TypeScriptBasic {
  value: number;
}

export function double(item: TypeScriptBasic): number {
  return item.value * 2;
}

export interface Service<T> {
  fetch(value: T): Promise<T>;
}

export declare function create<T>(value: T): Service<T>;

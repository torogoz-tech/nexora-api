import type { RemoteItem } from 'remote-items';
export interface Item { id: number; }
export type ItemId = string | number;
export enum Status { Ready, Done }
export class Cart extends Base { private count: number = 0; }
export async function fetchItem(id: number): Promise<Item> { const local = id; return await load(local); }
export const double = (value: number): number => { return value * 2; };
export const config: Record<string, number> = { retry: 3 };

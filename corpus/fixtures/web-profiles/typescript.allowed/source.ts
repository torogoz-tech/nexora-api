export interface Item { id: number; }
export type ItemId = string;
export enum Status { Ready }
export function getItem(id: ItemId): Item { return read(id); }

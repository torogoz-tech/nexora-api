import { target } from './target';
export function shadowCaller(target: (value: number) => number): number {
  return target(4);
}

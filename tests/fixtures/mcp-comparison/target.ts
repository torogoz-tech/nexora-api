export function target(value: number): number {
  if (value > 0) {
    return target(value - 1);
  }
  return 0;
}

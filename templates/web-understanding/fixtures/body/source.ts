export function compute(value: number): number {
  const local = value * 2;
  function helper(x: number): number { return x + 1; }
  { let nested = local + 1; helper(nested); }
  return helper(local)
}

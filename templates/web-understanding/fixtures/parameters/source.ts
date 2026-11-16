export function render(value, count) {
  function inner(value) { return `${value + 1}`; }
  const label = `${value + count}`;
  return inner(label);
}

export function View(value) {
  const label = `Value: ${format(value + 2)}`;
  return <Box title={label}>Literal text {`nested ${render(value)}`}</Box>;
}

'use client';
import React, { useState } from 'react';
export interface Props { label: string; }
export function Button({ label }: Props) {
  const [count, setCount] = useState(0);
  return (<button onClick={() => setCount(count + 1)}>{label}<span>{count}</span></button>);
}
export default Button;

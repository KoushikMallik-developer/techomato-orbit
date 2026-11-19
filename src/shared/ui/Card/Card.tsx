import type { HTMLAttributes } from 'react';

export function Card({ className = '', ...rest }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...rest}
      className={`rounded-card border border-border bg-paper p-5 shadow-card ${className}`}
    />
  );
}

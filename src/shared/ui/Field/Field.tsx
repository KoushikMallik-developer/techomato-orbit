import { Search } from 'lucide-react';
import type { InputHTMLAttributes, SelectHTMLAttributes } from 'react';

/** Shared look for text inputs and selects, so every screen matches. */
export const inputClass =
  'rounded-full border-[1.5px] border-border bg-paper px-4 py-1.5 text-sm outline-none transition hover:border-border-strong focus:border-fg';

export function SearchInput({ className = '', ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <span className={`relative inline-flex items-center ${className}`}>
      <Search aria-hidden className="pointer-events-none absolute left-3.5 size-4 text-fg-subtle" />
      <input {...rest} className={`${inputClass} w-full pl-10`} />
    </span>
  );
}

export function Select({ className = '', ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...rest} className={`${inputClass} ${className}`} />;
}

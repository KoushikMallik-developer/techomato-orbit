import { Search } from 'lucide-react';
import type { InputHTMLAttributes, SelectHTMLAttributes } from 'react';

/** Shared look for text inputs and selects, so every screen matches. */
export const inputClass =
  'rounded-full border-ui border-field-border bg-paper px-4 py-1.5 text-sm transition hover:border-fg-subtle focus:border-fg';

/** The same field look for the multi-line forms (service, project, database): a box, not a pill. */
export const formInputClass =
  'w-full rounded-xl border-ui border-field-border bg-canvas px-3 py-2 text-sm text-fg transition placeholder:text-fg-subtle hover:border-fg-subtle focus:border-fg';

/** Compact select used inside list rows. */
export const miniSelectClass =
  'rounded-full border-ui border-field-border bg-canvas px-3 py-1 text-xs';

/** A labelled checkbox row; the whole label is clickable. */
export function Check(props: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-center gap-2 py-0.5 text-sm">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => props.onChange(e.target.checked)}
      />
      {props.label}
    </label>
  );
}

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

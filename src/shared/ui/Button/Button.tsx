import type { ButtonHTMLAttributes } from 'react';
import { buttonSizes, buttonVariants, type ButtonSize, type ButtonVariant } from './Button.styles';

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
};

export function Button({ variant = 'ghost', size = 'md', className = '', ...rest }: Props) {
  return (
    <button
      type="button"
      {...rest}
      className={`inline-flex items-center justify-center gap-2 rounded-full border-[1.5px] font-semibold transition active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-50 ${buttonVariants[variant]} ${buttonSizes[size]} ${className}`}
    />
  );
}

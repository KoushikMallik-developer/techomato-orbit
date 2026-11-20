export const buttonVariants = {
  primary: 'bg-accent text-accent-fg border-accent hover:border-accent-hover hover:bg-accent-hover',
  ghost: 'bg-transparent text-fg border-fg/70 hover:bg-raised',
  danger: 'bg-transparent text-danger border-danger hover:bg-danger-soft',
} as const;

export const buttonSizes = {
  sm: 'min-h-8 px-3 text-sm',
  md: 'min-h-10 px-5 text-sm',
} as const;

export type ButtonVariant = keyof typeof buttonVariants;
export type ButtonSize = keyof typeof buttonSizes;

import type { ButtonHTMLAttributes } from 'react';

interface PixelButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary';
}

export function PixelButton({ variant = 'primary', className = '', ...props }: PixelButtonProps) {
  return (
    <button className={`pixel-button pixel-button--${variant} ${className}`.trim()} {...props} />
  );
}

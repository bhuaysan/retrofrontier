import { forwardRef, type ButtonHTMLAttributes } from 'react';

interface PixelButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary';
}

export const PixelButton = forwardRef<HTMLButtonElement, PixelButtonProps>(
  ({ variant = 'primary', className = '', ...props }, ref) => (
    <button
      ref={ref}
      className={`pixel-button pixel-button--${variant} ${className}`.trim()}
      {...props}
    />
  ),
);

PixelButton.displayName = 'PixelButton';

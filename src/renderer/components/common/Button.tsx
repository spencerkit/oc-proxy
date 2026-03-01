import React from 'react';
import type { LucideIcon } from 'lucide-react';
import { Loader2 } from 'lucide-react';
import styles from './Button.module.css';

export type ButtonVariant = 'primary' | 'danger' | 'ghost' | 'default';

export type ButtonSize = 'small' | 'medium' | 'large';

export interface ButtonProps extends Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, 'size'> {
  /**
   * Button variant
   * @default 'default'
   */
  variant?: ButtonVariant;

  /**
   * Button size
   * @default 'medium'
   */
  size?: ButtonSize;

  /**
   * Icon to display on the left
   */
  icon?: LucideIcon;

  /**
   * Icon to display on the right
   */
  iconRight?: LucideIcon;

  /**
   * Whether the button should take full width
   */
  fullWidth?: boolean;

  /**
   * Whether to show loading spinner
   */
  loading?: boolean;

  /**
   * Button content
   */
  children?: React.ReactNode;
}

/**
 * Button component with support for variants, icons, and styling
 */
export const Button: React.FC<ButtonProps> = ({
  variant = 'default',
  size = 'medium',
  icon: Icon,
  iconRight: IconRight,
  fullWidth = false,
  loading = false,
  disabled,
  className,
  children,
  ...props
}) => {
  const classes = [
    styles.base,
    variant !== 'default' && styles[variant],
    size !== 'medium' && styles[size],
    fullWidth && styles.fullWidth,
    loading && styles.loading,
    className,
  ].filter(Boolean).join(' ');

  return (
    <button
      className={classes}
      disabled={disabled || loading}
      {...props}
    >
      {loading ? (
        <span className={`${styles.icon} ${styles.iconLeft}`}>
          <Loader2 size={16} strokeWidth={2} className={styles.spinner} />
        </span>
      ) : Icon ? (
        <span className={`${styles.icon} ${styles.iconLeft}`}>
          <Icon size={16} strokeWidth={2} />
        </span>
      ) : null}
      {children}
      {IconRight && !loading && (
        <span className={`${styles.icon} ${styles.iconRight}`}>
          <IconRight size={16} strokeWidth={2} />
        </span>
      )}
    </button>
  );
};

export default Button;

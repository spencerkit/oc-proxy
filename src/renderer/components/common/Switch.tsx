import React from 'react';
import styles from './Switch.module.css';

export type SwitchSize = 'small' | 'medium' | 'large';

export interface SwitchProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size' | 'type' | 'onChange'> {
  /**
   * Switch size
   * @default 'medium'
   */
  size?: SwitchSize;

  /**
   * Whether the switch is checked
   */
  checked?: boolean;

  /**
   * Callback when checked state changes
   */
  onChange?: (checked: boolean) => void;

  /**
   * Label for the switch
   */
  label?: string;

  /**
   * Description/hint text displayed below the label
   */
  description?: string;

  /**
   * Whether the switch should take full width
   */
  fullWidth?: boolean;

  /**
   * Whether the switch is disabled
   */
  disabled?: boolean;
}

/**
 * Switch component for toggling boolean values
 */
export const Switch: React.FC<SwitchProps> = ({
  size = 'medium',
  checked = false,
  onChange,
  label,
  description,
  fullWidth = false,
  disabled = false,
  className,
  id,
  ...props
}) => {
  const switchId = id || `switch-${React.useId()}`;

  const switchClasses = [
    styles.switch,
    checked && styles.checked,
    disabled && styles.disabled,
    size !== 'medium' && styles[size],
    className,
  ].filter(Boolean).join(' ');

  const knobClasses = [
    styles.knob,
    checked && styles.checked,
  ].filter(Boolean).join(' ');

  const wrapperClasses = [
    styles.wrapper,
    fullWidth && styles.fullWidth,
  ].filter(Boolean).join(' ');

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    const newChecked = event.target.checked;
    onChange?.(newChecked);
  };

  return (
    <div className={wrapperClasses}>
      <label
        htmlFor={switchId}
        className={switchClasses}
      >
        <span className={knobClasses} />
        <input
          id={switchId}
          type="checkbox"
          className={styles.input}
          checked={checked}
          onChange={handleChange}
          disabled={disabled}
          {...props}
        />
      </label>
      {label && (
        <div>
          <span className={styles.label}>{label}</span>
          {description && (
            <span className={styles.description}>{description}</span>
          )}
        </div>
      )}
    </div>
  );
};

export default Switch;

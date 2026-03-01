import React, { useEffect, useCallback } from 'react';
import { X } from 'lucide-react';
import styles from './Modal.module.css';

export type ModalSize = 'small' | 'medium' | 'large' | 'full';

export interface ModalProps {
  /**
   * Whether the modal is visible
   */
  open: boolean;

  /**
   * Callback when modal is closed
   */
  onClose: () => void;

  /**
   * Modal title
   */
  title?: string;

  /**
   * Modal size
   * @default 'medium'
   */
  size?: ModalSize;

  /**
   * Whether to show close button
   * @default true
   */
  showClose?: boolean;

  /**
   * Whether clicking backdrop closes modal
   * @default true
   */
  closeOnBackdropClick?: boolean;

  /**
   * Whether pressing Escape closes modal
   * @default true
   */
  closeOnEscape?: boolean;

  /**
   * Modal body content
   */
  children?: React.ReactNode;

  /**
   * Modal footer content
   */
  footer?: React.ReactNode;

  /**
   * Additional CSS class for modal
   */
  className?: string;

  /**
   * Additional CSS class for backdrop
   */
  backdropClassName?: string;

  /**
   * Test ID for testing
   */
  testId?: string;
}

/**
 * Modal component for dialogs and focused interactions
 */
export const Modal: React.FC<ModalProps> = ({
  open,
  onClose,
  title,
  size = 'medium',
  showClose = true,
  closeOnBackdropClick = true,
  closeOnEscape = true,
  children,
  footer,
  className,
  backdropClassName,
  testId,
}) => {
  const modalRef = React.useRef<HTMLDivElement>(null);
  const previousActiveElementRef = React.useRef<HTMLElement | null>(null);
  const isExitingRef = React.useRef(false);

  // Focus management
  const focusModal = useCallback(() => {
    if (modalRef.current) {
      previousActiveElementRef.current = document.activeElement as HTMLElement;
      modalRef.current.focus();
    }
  }, []);

  const restoreFocus = useCallback(() => {
    if (previousActiveElementRef.current) {
      previousActiveElementRef.current.focus();
    }
  }, []);

  // Handle escape key
  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (closeOnEscape && event.key === 'Escape' && open) {
        event.preventDefault();
        onClose();
      }
    },
    [closeOnEscape, open, onClose]
  );

  // Handle backdrop click
  const handleBackdropClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      if (closeOnBackdropClick && event.target === event.currentTarget) {
        onClose();
      }
    },
    [closeOnBackdropClick, onClose]
  );

  // Setup event listeners and focus
  useEffect(() => {
    if (open) {
      isExitingRef.current = false;
      focusModal();
      document.addEventListener('keydown', handleKeyDown);
      // Prevent body scroll
      document.body.style.overflow = 'hidden';
    } else {
      restoreFocus();
      document.removeEventListener('keydown', handleKeyDown);
      // Restore body scroll
      document.body.style.overflow = '';
    }

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      document.body.style.overflow = '';
    };
  }, [open, focusModal, restoreFocus, handleKeyDown]);

  // Handle trap focus within modal
  useEffect(() => {
    if (!open || !modalRef.current) return;

    const handleTabKey = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;

      const focusableElements = modalRef.current?.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );

      if (!focusableElements || focusableElements.length === 0) return;

      const firstElement = focusableElements[0] as HTMLElement;
      const lastElement = focusableElements[focusableElements.length - 1] as HTMLElement;

      if (event.shiftKey) {
        if (document.activeElement === firstElement) {
          event.preventDefault();
          lastElement.focus();
        }
      } else {
        if (document.activeElement === lastElement) {
          event.preventDefault();
          firstElement.focus();
        }
      }
    };

    document.addEventListener('keydown', handleTabKey);
    return () => document.removeEventListener('keydown', handleTabKey);
  }, [open]);

  if (!open) return null;

  const modalClasses = [
    styles.modal,
    size !== 'medium' && styles[size],
    className,
  ].filter(Boolean).join(' ');

  const backdropClasses = [
    styles.backdrop,
    isExitingRef.current && styles.exiting,
    backdropClassName,
  ].filter(Boolean).join(' ');

  return (
    <div
      className={backdropClasses}
      onClick={handleBackdropClick}
      role="presentation"
    >
      <div
        ref={modalRef}
        className={modalClasses}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? `modal-title-${testId || 'default'}` : undefined}
        tabIndex={-1}
        data-testid={testId}
      >
        {title && (
          <div className={styles.header}>
            <h2 id={`modal-title-${testId || 'default'}`} className={styles.title}>
              {title}
            </h2>
            {showClose && (
              <button
                type="button"
                className={styles.closeButton}
                onClick={onClose}
                aria-label="Close modal"
              >
                <X size={18} strokeWidth={2} />
              </button>
            )}
          </div>
        )}

        {children && <div className={styles.body}>{children}</div>}

        {footer && <div className={styles.footer}>{footer}</div>}
      </div>
    </div>
  );
};

export default Modal;

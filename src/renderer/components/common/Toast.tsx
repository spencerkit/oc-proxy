import React, { useCallback, createContext, useContext } from 'react';
import { Check, X, AlertCircle, AlertTriangle, Info } from 'lucide-react';
import styles from './Toast.module.css';

export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastData {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
  actions?: ToastAction[];
  onClose?: () => void;
}

interface ToastContextType {
  toast: (data: Omit<ToastData, 'id'>) => void;
  success: (message: string, duration?: number, actions?: ToastAction[]) => void;
  error: (message: string, duration?: number, actions?: ToastAction[]) => void;
  warning: (message: string, duration?: number, actions?: ToastAction[]) => void;
  info: (message: string, duration?: number, actions?: ToastAction[]) => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

/**
 * Toast Provider component
 */
export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = React.useState<ToastData[]>([]);

  const addToast = useCallback((data: Omit<ToastData, 'id'>) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const toast: ToastData = { ...data, id };

    setToasts((prev) => [...prev, toast]);

    // Auto dismiss if duration is provided
    if (data.duration !== Infinity) {
      const duration = data.duration ?? 4000;
      setTimeout(() => {
        removeToast(id);
      }, duration);
    }
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const toast = useCallback(
    (data: Omit<ToastData, 'id'>) => {
      addToast(data);
    },
    [addToast]
  );

  const success = useCallback(
    (message: string, duration?: number, actions?: ToastAction[]) => {
      addToast({ type: 'success', message, duration, actions });
    },
    [addToast]
  );

  const error = useCallback(
    (message: string, duration?: number, actions?: ToastAction[]) => {
      addToast({ type: 'error', message, duration, actions });
    },
    [addToast]
  );

  const warning = useCallback(
    (message: string, duration?: number, actions?: ToastAction[]) => {
      addToast({ type: 'warning', message, duration, actions });
    },
    [addToast]
  );

  const info = useCallback(
    (message: string, duration?: number, actions?: ToastAction[]) => {
      addToast({ type: 'info', message, duration, actions });
    },
    [addToast]
  );

  const contextValue: ToastContextType = {
    toast,
    success,
    error,
    warning,
    info,
  };

  return (
    <ToastContext.Provider value={contextValue}>
      {children}
      <ToastContainer toasts={toasts} onRemove={removeToast} />
    </ToastContext.Provider>
  );
};

/**
 * Hook to use toast functionality
 */
export const useToast = (): ToastContextType => {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return context;
};

/**
 * Toast Container component
 */
interface ToastContainerProps {
  toasts: ToastData[];
  onRemove: (id: string) => void;
}

const ToastContainer: React.FC<ToastContainerProps> = ({ toasts, onRemove }) => {
  return (
    <div className={styles.container} role="status" aria-live="polite">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onRemove={onRemove} />
      ))}
    </div>
  );
};

/**
 * Individual Toast Item component
 */
interface ToastItemProps {
  toast: ToastData;
  onRemove: (id: string) => void;
}

const ToastItem: React.FC<ToastItemProps> = ({ toast, onRemove }) => {
  const { id, type, message, duration, actions, onClose } = toast;

  const handleClose = useCallback(() => {
    onClose?.();
    onRemove(id);
  }, [id, onClose, onRemove]);

  // Get icon based on toast type
  const Icon = React.useMemo(() => {
    switch (type) {
      case 'success':
        return Check;
      case 'error':
        return AlertCircle;
      case 'warning':
        return AlertTriangle;
      case 'info':
        return Info;
      default:
        return Info;
    }
  }, [type]);

  return (
    <div
      className={`${styles.toast} ${styles[type]}`}
      role="alert"
      style={
        duration !== undefined && duration !== Infinity
          ? { '--progress-duration': `${duration}ms` } as React.CSSProperties
          : undefined
      }
    >
      <span className={styles.icon}>
        <Icon size={16} strokeWidth={2} />
      </span>
      <span className={styles.content}>{message}</span>
      {actions && actions.length > 0 && (
        <div className={styles.actions}>
          {actions.map((action, index) => (
            <button
              key={index}
              type="button"
              className={styles.actionButton}
              onClick={action.onClick}
            >
              {action.label}
            </button>
          ))}
        </div>
      )}
      <button
        type="button"
        className={styles.closeButton}
        onClick={handleClose}
        aria-label="Close notification"
      >
        <X size={14} strokeWidth={2} />
      </button>
      {duration !== undefined && duration !== Infinity && (
        <div className={styles.progressBar} />
      )}
    </div>
  );
};

/**
 * Toast component for displaying notifications
 * This is a convenience component that can be used directly
 */
export const Toast: React.FC = () => {
  return null; // ToastContainer is rendered by ToastProvider
};

export default Toast;

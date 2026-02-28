/**
 * useTheme Hook
 *
 * Custom hook for managing application theme.
 * Provides theme selector, setTheme action, and effect to apply theme to document.
 */

import { useEffect } from 'react';

/**
 * Theme type
 */
export type Theme = 'light' | 'dark' | 'system';

/**
 * Local storage key for theme preference
 */
const THEME_STORAGE_KEY = 'oc-proxy-theme';

/**
 * Default theme
 */
const DEFAULT_THEME: Theme = 'system';

/**
 * Get the initial theme from local storage or default
 *
 * @returns The initial theme
 */
function getInitialTheme(): Theme {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    if (stored === 'light' || stored === 'dark' || stored === 'system') {
      return stored;
    }
  } catch {
    // Ignore localStorage errors (e.g., in restricted environments)
  }
  return DEFAULT_THEME;
}

/**
 * Get the effective theme (resolving 'system' preference)
 *
 * @param theme - The theme preference
 * @returns The effective theme ('light' or 'dark')
 */
export function getEffectiveTheme(theme: Theme): 'light' | 'dark' {
  if (theme === 'system') {
    // Check system preference via window.matchMedia
    if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
      return 'dark';
    }
    return 'light';
  }
  return theme;
}

/**
 * Apply theme to document
 * Adds/removes dark class from document element
 *
 * @param theme - The theme to apply
 */
export function applyThemeToDocument(theme: Theme): void {
  const effectiveTheme = getEffectiveTheme(theme);
  const documentElement = document.documentElement;

  if (effectiveTheme === 'dark') {
    documentElement.classList.add('dark');
  } else {
    documentElement.classList.remove('dark');
  }

  // Set CSS custom property for potential styling
  documentElement.style.setProperty('--color-scheme', effectiveTheme);
}

/**
 * Save theme preference to local storage
 *
 * @param theme - The theme to save
 */
function saveTheme(theme: Theme): void {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Ignore localStorage errors
  }
}

/**
 * Current theme state (singleton)
 */
let currentTheme: Theme = getInitialTheme();

/**
 * Apply initial theme on load
 */
applyThemeToDocument(currentTheme);

/**
 * Set the current theme and apply it to the document
 *
 * @param theme - The theme to set
 */
export function setTheme(theme: Theme): void {
  currentTheme = theme;
  saveTheme(theme);
  applyThemeToDocument(theme);
}

/**
 * Get the current theme preference
 *
 * @returns The current theme
 */
export function getTheme(): Theme {
  return currentTheme;
}

/**
 * Hook for managing application theme
 * Provides theme value and setter, with automatic application to document
 *
 * @returns Object containing theme, setTheme, and isDark
 *
 * @example
 * function ThemeToggle() {
 *   const { theme, setTheme, isDark } = useTheme();
 *
 *   return (
 *     <button onClick={() => setTheme(isDark ? 'light' : 'dark')}>
 *       {isDark ? 'Light' : 'Dark'} Mode
 *     </button>
 *   );
 * }
 */
export function useTheme() {
  const isDark = getEffectiveTheme(currentTheme) === 'dark';

  // Apply theme whenever it changes
  useEffect(() => {
    applyThemeToDocument(currentTheme);
  }, [currentTheme]);

  return {
    theme: currentTheme,
    setTheme,
    isDark,
  };
}

/**
 * Hook for accessing only the current theme value
 * Useful when you only need to read the theme
 *
 * @returns The current theme
 *
 * @example
 * const theme = useThemeValue();
 * const bgColor = theme === 'dark' ? '#1a1a1a' : '#ffffff';
 */
export function useThemeValue(): Theme {
  return currentTheme;
}

/**
 * Hook for checking if dark mode is active
 * Useful for conditional rendering based on theme
 *
 * @returns Boolean indicating if dark mode is active
 *
 * @example
 * const isDark = useIsDarkMode();
 *
 * return <div style={{ background: isDark ? '#333' : '#fff' }} />;
 */
export function useIsDarkMode(): boolean {
  return getEffectiveTheme(currentTheme) === 'dark';
}

/**
 * Hook for listening to system theme changes
 * Automatically updates theme when system preference changes
 * Only applies when theme is set to 'system'
 *
 * @example
 * function App() {
 *   useSystemThemeListener();
 *   // ... rest of app
 * }
 */
export function useSystemThemeListener() {
  useEffect(() => {
    // Only listen if theme is set to 'system'
    if (currentTheme !== 'system') {
      return;
    }

    // Check for matchMedia support
    if (!window.matchMedia) {
      return;
    }

    // Create media query listener
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

    // Define change handler
    const handleChange = () => {
      applyThemeToDocument(currentTheme);
    };

    // Add listener
    mediaQuery.addEventListener('change', handleChange);

    // Cleanup
    return () => {
      mediaQuery.removeEventListener('change', handleChange);
    };
  }, []);
}

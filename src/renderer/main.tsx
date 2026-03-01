import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import { I18nextProvider } from 'react-i18next';
import i18n, { initI18n, type Locale } from './i18n';
import { ToastProvider } from './contexts/ToastContext';
import { ToastContainer } from './components/common/ToastContainer';
import './styles.css';

// 添加全局错误处理
window.onerror = (message, source, lineno, colno, error) => {
  console.error('Global error:', { message, source, lineno, colno, error });
  return false;
};

window.onunhandledrejection = (event) => {
  console.error('Unhandled promise rejection:', event.reason);
};

console.log('Renderer starting...');
console.log('window.proxyApp:', window.proxyApp);

function resolveTheme(theme?: unknown): 'light' | 'dark' {
  if (theme === 'light' || theme === 'dark') {
    return theme;
  }

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function resolveLocale(locale?: unknown): Locale {
  return locale === 'zh-CN' ? 'zh-CN' : 'en-US';
}

// Initialize i18n before rendering
async function init() {
  let initialLocale: Locale = 'en-US';
  let initialTheme: 'light' | 'dark' = resolveTheme();

  try {
    const config = await window.proxyApp.getConfig();
    initialLocale = resolveLocale(config?.ui?.locale);
    initialTheme = resolveTheme(config?.ui?.theme);
  } catch (error) {
    console.error('[Main] Failed to load config for bootstrap preferences:', error);
  }

  document.documentElement.setAttribute('data-theme', initialTheme);

  console.log('[Main] Initializing i18n...');
  await initI18n(initialLocale);
  console.log('[Main] i18n initialized');

  const rootElement = document.getElementById('root');
  if (!rootElement) {
    console.error('Root element not found!');
    throw new Error('Root element not found');
  }

  console.log('[Main] Root element found, rendering...');

  ReactDOM.createRoot(rootElement).render(
    <React.StrictMode>
      <I18nextProvider i18n={i18n}>
        <ToastProvider>
          <BrowserRouter>
            <App />
            <ToastContainer />
          </BrowserRouter>
        </ToastProvider>
      </I18nextProvider>
    </React.StrictMode>
  );

  console.log('[Main] React rendered');
}

init().catch(console.error);

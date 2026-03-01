# UI 优化实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 对 OA Proxy 桌面应用进行全面 UI 优化，包括服务状态控制、组件样式标准化、交互优化

**Architecture:** 基于现有 React + CSS Modules 技术栈，优化 Zustand store 添加服务控制方法，改进 Header/ServicePage 组件

**Tech Stack:** React 18, CSS Modules, Zustand, TypeScript

---

## Task 1: Store 添加服务控制方法

**Files:**
- Modify: `src/renderer/store/proxyStore.ts`

**Step 1: 添加 startServer 和 stopServer 方法**

在 proxyStore.ts 的 interface ProxyState 中添加:
```typescript
startServer: () => Promise<void>;
stopServer: () => Promise<void>;
```

在 create<ProxyState> 中添加实现:
```typescript
startServer: async () => {
  try {
    set({ loading: true, error: null });
    const status = await ipc.startServer();
    set({ status, loading: false });
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : 'Failed to start server';
    set({ error: errorMessage, loading: false });
  }
},

stopServer: async () => {
  try {
    set({ loading: true, error: null });
    const status = await ipc.stopServer();
    set({ status, loading: false });
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : 'Failed to stop server';
    set({ error: errorMessage, loading: false });
  }
},
```

**Step 2: 提交代码**

```bash
git add src/renderer/store/proxyStore.ts
git commit -m "feat: add startServer and stopServer to proxy store"
```

---

## Task 2: Header 添加服务状态显示

**Files:**
- Modify: `src/renderer/components/layout/Header.tsx`
- Modify: `src/renderer/components/layout/Header.module.css`

**Step 1: 在 Header.tsx 中添加服务状态区域**

在 left 部分添加服务状态显示:
```typescript
// 在 HeaderProps 中添加
interface HeaderProps {
  // ... existing
  isRunning?: boolean;
  serverAddress?: string;
  onStartServer?: () => void;
  onStopServer?: () => void;
}

// 在 Header 组件中添加状态显示
<div className={styles.serviceStatus}>
  <span className={`${styles.statusDot} ${isRunning ? styles.running : styles.stopped}`} />
  <span className={styles.statusText}>
    {isRunning ? t('header.serviceRunning') : t('header.serviceStopped')}
  </span>
  {serverAddress && (
    <span className={styles.serverAddress}>{serverAddress}</span>
  )}
</div>
```

**Step 2: 添加按钮控制服务**

```typescript
<Button
  variant={isRunning ? 'danger' : 'primary'}
  size="small"
  onClick={isRunning ? onStopServer : onStartServer}
>
  {isRunning ? t('header.stop') : t('header.start')}
</Button>
```

**Step 3: 添加样式**

在 Header.module.css 中添加:
```css
.serviceStatus {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.statusDot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.statusDot.running {
  background: #22c55e;
  box-shadow: 0 0 6px #22c55e;
}

.statusDot.stopped {
  background: #94a3b8;
}

.statusText {
  font-size: var(--font-size-sm);
  color: var(--text);
  font-weight: var(--font-weight-medium);
}

.serverAddress {
  font-size: var(--font-size-xs);
  color: var(--muted);
  font-family: var(--font-mono);
}
```

**Step 4: 提交代码**

```bash
git add src/renderer/components/layout/Header.tsx src/renderer/components/layout/Header.module.css
git commit -m "feat: add service status display and control to header"
```

---

## Task 3: Layout 传递服务状态到 Header

**Files:**
- Modify: `src/renderer/components/layout/Layout.tsx`

**Step 1: 修改 Layout 组件**

```typescript
// 在 LayoutProps 中添加
interface LayoutProps {
  // ... existing
  isRunning?: boolean;
  serverAddress?: string;
  onStartServer?: () => void;
  onStopServer?: () => void;
}

// 修改 Layout 组件实现
export const Layout: React.FC<LayoutProps> = ({
  // ... existing props
  isRunning,
  serverAddress,
  onStartServer,
  onStopServer,
}) => {
  // ... existing code

  return (
    <div className={styles.layout}>
      <Header
        view={currentView}
        isRunning={isRunning}
        serverAddress={serverAddress}
        onStartServer={onStartServer}
        onStopServer={onStopServer}
        {...header}
      />
      // ... rest of component
    </div>
  );
};
```

**Step 2: 提交代码**

```bash
git add src/renderer/components/layout/Layout.tsx
git commit -m "feat: pass service status props to header"
```

---

## Task 4: App 组件连接服务状态

**Files:**
- Modify: `src/renderer/App.tsx`

**Step 1: 添加服务控制逻辑**

```typescript
const App: React.FC = () => {
  // ... existing code
  const { init, loading, error, status, startServer, stopServer } = store || {
    init: () => {},
    loading: false,
    error: null,
    status: null,
    startServer: () => {},
    stopServer: () => {}
  };

  const isRunning = status?.running ?? false;
  const serverAddress = status?.address
    ? `http://${status.address}:${status.port}`
    : undefined;

  // ... rest of code

  return (
    <Layout
      isRunning={isRunning}
      serverAddress={serverAddress}
      onStartServer={startServer}
      onStopServer={stopServer}
    >
      // ... routes
    </Layout>
  );
};
```

**Step 2: 提交代码**

```bash
git add src/renderer/App.tsx
git commit -m "feat: connect service status to layout and header"
```

---

## Task 5: Button 组件样式标准化

**Files:**
- Modify: `src/renderer/components/common/Button.module.css`

**Step 1: 优化 Button 样式**

更新 Button.module.css:
```css
/* 统一样式变量 */
.button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  font-family: var(--font-sans);
  font-weight: var(--font-weight-medium);
  cursor: pointer;
  transition: all var(--transition-fast);
  outline: none;
  white-space: nowrap;
}

/* 尺寸 */
.small {
  height: 28px;
  padding: 0 var(--space-3);
  font-size: var(--font-size-sm);
}

.medium {
  height: 36px;
  padding: 0 var(--space-4);
  font-size: var(--font-size-md);
}

.large {
  height: 44px;
  padding: 0 var(--space-5);
  font-size: var(--font-size-lg);
}

/* 变体 */
.primary {
  background: var(--accent);
  color: white;
  border-color: var(--accent);
}

.primary:hover:not(:disabled) {
  background: var(--accent-ink);
  border-color: var(--accent-ink);
}

.primary:active:not(:disabled) {
  transform: translateY(1px);
}

.default {
  background: transparent;
  color: var(--text);
  border-color: var(--line);
}

.default:hover:not(:disabled) {
  background: var(--tab-active-bg-2);
  border-color: var(--line-strong);
}

.danger {
  background: var(--danger);
  color: white;
  border-color: var(--danger);
}

.danger:hover:not(:disabled) {
  background: #a33a3a;
  border-color: #a33a3a;
}

.ghost {
  background: transparent;
  color: var(--text);
  border-color: transparent;
}

.ghost:hover:not(:disabled) {
  background: var(--tab-active-bg-2);
}

/* 禁用状态 */
.button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
```

**Step 2: 提交代码**

```bash
git add src/renderer/components/common/Button.module.css
git commit -m "style: standardize button component styles"
```

---

## Task 6: Input 组件样式优化

**Files:**
- Modify: `src/renderer/components/common/Input.module.css`

**Step 1: 优化 Input 样式**

```css
.base {
  display: block;
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-md);
  font-family: var(--font-sans);
  font-size: var(--font-size-md);
  font-weight: var(--font-weight-normal);
  color: var(--text);
  background: var(--panel-strong);
  transition: all var(--transition-fast);
  outline: none;
}

.base::placeholder {
  color: var(--muted);
}

.base:hover:not(:disabled) {
  border-color: var(--btn-hover-border);
}

.base:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px rgba(12, 139, 115, 0.15);
}
```

**Step 2: 提交代码**

```bash
git add src/renderer/components/common/Input.module.css
git commit -style: optimize input component styles"
```

---

## Task 7: 导航 Tab 样式优化

**Files:**
- Modify: `src/renderer/components/layout/Header.module.css`

**Step 1: 更新导航样式**

```css
.navButton {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  cursor: pointer;
  font-family: var(--font-sans);
  font-size: var(--font-size-sm);
  font-weight: var(--font-weight-medium);
  color: var(--muted);
  transition: all var(--transition-fast);
  outline: none;
}

.navButton:hover {
  color: var(--text);
  background: var(--tab-active-bg-2);
}

.navButton.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
  background: var(--tab-active-bg-1);
}
```

**Step 2: 移除 divider 样式**

简化导航区域，移除不必要的分割线。

**Step 3: 提交代码**

```bash
git add src/renderer/components/layout/Header.module.css
git commit -m "style: optimize navigation tab styles"
```

---

## Task 8: ServicePage 规则列表添加删除按钮

**Files:**
- Modify: `src/renderer/pages/ServicePage/RuleList.tsx`
- Modify: `src/renderer/pages/ServicePage/ServicePage.module.css`

**Step 1: 查看 RuleList 组件**

先读取 RuleList.tsx 了解当前结构。

**Step 2: 在每行添加删除操作**

在规则列表的每行添加操作列，包含删除按钮。

**Step 3: 提交代码**

```bash
git add src/renderer/pages/ServicePage/RuleList.tsx src/renderer/pages/ServicePage/ServicePage.module.css
git commit -m "feat: add delete button to each rule row"
```

---

## Task 9: ServicePage 布局优化

**Files:**
- Modify: `src/renderer/pages/ServicePage/ServicePage.module.css`

**Step 1: 优化整体布局样式**

- 侧边栏宽度调整
- 主内容区域间距优化
- 卡片/区块圆角和阴影统一

**Step 2: 提交代码**

```bash
git add src/renderer/pages/ServicePage/ServicePage.module.css
git commit -m "style: optimize service page layout"
```

---

## Task 10: 整体样式微调

**Files:**
- Modify: `src/renderer/styles/global.css`
- Modify: `src/renderer/styles/variables.css`

**Step 1: 根据实际效果微调**

- 检查颜色变量是否需要调整
- 确保间距系统一致

**Step 2: 提交代码**

```bash
git add src/renderer/styles/global.css src/renderer/styles/variables.css
git commit -m "style: fine-tune global styles"
```

---

## 执行方式

**Plan complete and saved to `docs/plans/2026-03-01-ui-optimization-design.md`. Two execution options:**

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

**Which approach?**

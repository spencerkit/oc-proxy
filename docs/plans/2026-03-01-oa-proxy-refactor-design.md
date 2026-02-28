# OA Proxy 重构设计文档

## 概述

将现有的 Electron + Vanilla JS 应用重构为 Electron + Vite + React + TypeScript 架构，优化 UI/UX，扩展功能。

## 技术栈

- **Electron** - 桌面应用框架
- **Vite** - 构建工具和开发服务器
- **React 18+** - UI 框架
- **TypeScript** - 类型安全
- **React Router v6** - 路由管理
- **Lucide React** - 图标库
- **CSS Modules** - 样式方案
- **i18next** - 国际化（支持中英文）
- **Zustand** - 轻量级状态管理

## 项目结构

```
oc-proxy/
├── src/
│   ├── main/                    # Electron 主进程
│   │   ├── index.ts             # 入口
│   │   ├── preload.ts           # 预加载脚本
│   │   └── logStore.ts         # 日志存储
│   ├── renderer/                # React 渲染进程
│   │   ├── main.tsx            # React 入口
│   │   ├── App.tsx             # 根组件
│   │   ├── components/          # 组件
│   │   │   ├── Layout/
│   │   │   │   ├── Header.tsx
│   │   │   │   ├── Header.module.css
│   │   │   │   └── index.ts
│   │   │   ├── ServicePage/
│   │   │   │   ├── GroupTabs.tsx
│   │   │   │   ├── GroupTabs.module.css
│   │   │   │   ├── RuleCard.tsx
│   │   │   │   ├── RuleCard.module.css
│   │   │   │   ├── RuleEdit.tsx
│   │   │   │   ├── RuleEdit.module.css
│   │   │   │   └── index.ts
│   │   │   ├── SettingsPage/
│   │   │   │   ├── SettingsPage.tsx
│   │   │   │   ├── SettingsPage.module.css
│   │   │   │   └── index.ts
│   │   │   ├── LogsPage/
│   │   │   │   ├── LogsView.tsx
│   │   │   │   ├── LogsView.module.css
│   │   │   │   └── index.ts
│   │   │   └── common/
│   │   │       ├── Button.tsx
│   │   │       ├── Button.module.css
│   │   │       ├── Input.tsx
│   │   │       ├── Switch.tsx
│   │   │       ├── Modal.tsx
│   │   │       ├── Toast.tsx
│   │   │       └── ErrorBoundary.tsx
│   │   ├── hooks/              # 自定义 Hooks
│   │   │   ├── useProxyConfig.ts
│   │   │   ├── useProxyStatus.ts
│   │   │   ├── useLogs.ts
│   │   │   ├── useTranslation.ts
│   │   │   └── useTheme.ts
│   │   ├── store/              # 状态管理
│   │   │   └── proxyStore.ts
│   │   ├── types/              # 类型定义
│   │   │   ├── config.ts
│   │   │   ├── proxy.ts
│   │   │   └── index.ts
│   │   ├── i18n/               # 国际化
│   │   │   ├── index.ts
│   │   │   ├── zh-CN.ts
│   │   │   └── en-US.ts
│   │   ├── theme/              # 主题
│   │   │   ├── index.ts
│   │   │   ├── light.css
│   │   │   └── dark.css
│   │   ├── utils/              # 工具函数
│   │   │   ├── index.ts
│   │   │   └── ipc.ts
│   │   ├── styles/             # 全局样式
│   │   │   ├── variables.css
│   │   │   ├── global.css
│   │   │   └── reset.css
│   │   └── vite-env.d.ts
│   └── proxy/                 # 后端代理逻辑（保持不变）
├── docs/
│   └── plans/
│       └── 2026-03-01-oa-proxy-refactor-design.md
├── package.json
├── tsconfig.json
├── vite.config.ts
└── electron-builder.json
```

## 路由设计

```tsx
<BrowserRouter>
  <Routes>
    <Route path="/" element={<Layout />}>
      <Route index element={<ServicePage />} />
      <Route path="settings" element={<SettingsPage />} />
      <Route path="logs" element={<LogsPage />} />
      <Route path="rule/:ruleId" element={<RuleEdit />} />
    </Route>
  </Routes>
</BrowserRouter>
```

## UI 设计规范

### 字体系统

- **字重**：默认 300（Light）
- **大标题**：最大 500（Medium）
- **字体大小**：整体缩小 10-15%

### 颜色系统

```css
--bg-a: #f4f9ff;
--bg-b: #fff7ee;
--bg-c: #f8fffa;
--text: #142033;
--muted: #5f6f8a;
--accent: #0c8b73;
--danger: #c14a4a;
```

### ServicePage 布局

分组 Tabs 下方直接展示当前选中分组的规则列表，不需要独立的分组详情页。

### 规则卡片设计

**列表视图（仅展示基本信息）：**
- 模型名称
- 转发方向
- 生效状态
- 点击卡片展开/进入编辑模式

## 功能设计

### 1. 国际化（i18n）

支持中英文切换，通过设置页面配置。

```tsx
// 使用示例
const { t } = useTranslation();
<h1>{t('app.title')}</h1>
```

### 2. 主题切换

支持浅色/深色/跟随系统三种模式。

```tsx
const { theme, setTheme } = useTheme();
```

### 3. 开机启动

在设置页面添加开机启动配置选项。

### 4. 设置页面配置项

所有配置项包含 tips 说明：

- **监听 Host** - 服务监听的地址，0.0.0.0 表示监听所有网卡
- **服务端口 (1-65535)** - HTTP 服务监听端口，请确保端口未被占用
- **严格模式** - 启用后，不兼容的字段会直接报错而非忽略
- **语言** - 切换界面显示语言
- **主题** - 选择界面主题颜色风格
- **开机启动** - 系统启动时自动运行代理服务

## 数据流设计

```
Renderer Process
     ↓
useProxyConfig (Hook) ←→ IPC → Main Process
     ↓
Zustand Store
     ↓
Components (消费状态)
```

## 迁移策略

采用**直接替换**方式，不保留旧代码：

1. 初始化 Vite + React + TypeScript 项目配置
2. 迁移类型定义到 `types/`
3. 迁移 UI 组件到 React
4. 设置路由和状态管理
5. 实现国际化功能
6. 实现主题切换功能
7. 添加开机启动配置
8. 测试功能完整性
9. 清理旧代码

## 构建配置

### Vite 配置

```typescript
// vite.config.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  base: './',
  build: {
    outDir: '../out/renderer',
    emptyOutDir: true,
  },
  plugins: [react()],
});
```

### TypeScript 配置

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "paths": {
      "@/*": ["./src/renderer/*"]
    }
  }
}
```

## 后端保留逻辑

以下模块保持不变，仅调整类型定义：

- `src/proxy/configStore.js`
- `src/proxy/server.js`
- `src/proxy/ruleEngine.js`
- `src/proxy/mappers/`
- `src/main/main.js`（转换为 TypeScript）
- `src/main/logStore.js`（转换为 TypeScript）

## 待实现功能清单

- [ ] Vite + React + TypeScript 项目初始化
- [ ] 类型定义迁移
- [ ] 基础组件（Button, Input, Modal, Toast, Switch）
- [ ] Layout 组件（Header）
- [ ] ServicePage（分组 Tabs + 规则列表）
- [ ] RuleEdit（规则编辑页面）
- [ ] SettingsPage（设置页面，含新配置项）
- [ ] LogsPage（日志页面）
- [ ] React Router 配置
- [ ] Zustand 状态管理
- [ ] 国际化功能（中英文切换）
- [ ] 主题切换功能
- [ ] 开机启动配置
- [ ] 错误边界和错误处理
- [ ] CSS Modules 样式系统
- [ ] 主进程 TypeScript 迁移
- [ ] 构建配置和测试

/**
 * Chinese (Simplified) Translations for OA Proxy
 */

export const zhCN = {
  // App
  app: {
    title: 'OA Proxy',
    protocolForwardingService: '协议中转服务',
    statusLoading: '状态加载中...',
  },

  // Header
  header: {
    serviceSwitch: '服务开关',
    addGroup: '添加分组',
    settings: '设置',
    logs: '日志',
    backToService: '返回服务',
    serviceRunning: '运行中',
    serviceStopped: '已停止',
    start: '启动',
    stop: '停止',
  },

  // Service Status
  service: {
    running: '运行中',
    stopped: '已停止',
    port: '端口',
    host: '主机',
    requests: '请求',
    errors: '错误',
    avgLatency: '平均延迟',
    start: '启动',
    stop: '停止',
    statusText: '{{running}} | {{host}}:{{port}} | 请求 {{requests}} | 错误 {{errors}} | 平均延迟 {{latency}}ms',
    stoppedText: '已停止 | {{host}}:{{port}}',
  },

  // Service Page / Groups
  servicePage: {
    noGroupsHint: '暂无分组，请先点击"添加分组"。',
    noGroupSelected: '请选择一个分组。',
    noRulesHint: '该分组暂无规则，请点击"添加规则"。',
    groupPath: '分组 Path',
    entryUrl: '入口 URL',
    copyEntryUrl: '复制入口 URL',
    addRule: '添加规则',
    deleteGroup: '删除分组',
    groupName: '分组名称',
    model: '模型名称',
    forwardDirection: '转发方向',
    token: 'Token',
    apiAddress: 'API 地址',
    current: '当前生效',
    saveRule: '保存规则',
    deleteRule: '删除',
  },

  // Rule Directions
  ruleDirection: {
    oc: 'OpenAI -> Anthropic',
    co: 'Anthropic -> OpenAI',
  },

  // Settings
  settings: {
    title: '服务设置',
    listenHost: '监听 Host',
    servicePort: '服务端口',
    strictMode: '严格模式（不兼容字段直接报错）',
    save: '保存',
    cancel: '取消',
    saveSuccess: '设置已保存',
    portError: '端口必须是 1-65535 的整数',
  },

  // Logs
  logs: {
    title: '请求链路日志',
    recentLogs: '最近 {{count}} 条',
    refresh: '刷新',
    clear: '清空',
    noLogs: '暂无日志',
    refreshSuccess: '日志已刷新',
    clearSuccess: '日志已清空',
    refreshError: '日志刷新失败',
    // Log entries
    request: '请求',
    status: '状态',
    requestStatus: 'HTTP {{status}} | {{state}}',
    errorReason: ' | 原因：{{reason}}',
    requestBody: '请求体',
    forwardingTo: '准备转发到',
    notForwarding: '未进入转发（可能是路径/鉴权/规则校验失败）',
    separator: '----------------------------------------',
  },

  // Modals
  modal: {
    addGroupTitle: '添加分组',
    groupNameLabel: '分组名称',
    groupNamePlaceholder: '例如 claude',
    pathLabel: '转发 Path',
    pathPlaceholder: '例如 claude',
    pathHint: '请求路径示例：`/oc/{{path}}`（具体转发方向由生效规则决定）',
    create: '创建',
    cancel: '取消',
    save: '保存',
  },

  // Delete Group Modal
  deleteGroupModal: {
    title: '删除分组确认',
    confirmText: '将删除分组"{{name}}"（path: {{path}}）。该操作不可撤销，确认继续吗？',
    confirmDelete: '确认删除',
    cancel: '取消',
  },

  // Toast Messages
  toast: {
    serviceStarted: '服务已启动',
    serviceStopped: '服务已停止',
    restartComplete: '重启完成',
    groupCreated: '分组已创建',
    groupDeleted: '分组已删除',
    ruleCreated: '规则已创建',
    ruleUpdated: '规则已更新',
    ruleSaved: '规则已保存',
    ruleDeleted: '规则已删除',
    ruleSwitched: '已切换生效规则',
    entryUrlCopied: '入口 URL 已复制',
    copyFailed: '复制失败',
    configSaved: '配置已保存',
    ruleNotFound: '规则不存在',
    groupNotFound: '分组不存在',
  },

  // Rule Edit Page
  ruleEditPage: {
    title: '编辑规则',
    saveChanges: '保存修改',
  },

  // Rule Create Page
  ruleCreatePage: {
    title: '创建新规则',
    newRule: '新规则',
    createRule: '创建规则',
  },

  // Common
  common: {
    loading: '加载中...',
    error: '错误',
    success: '成功',
    failed: '失败',
    confirm: '确认',
    cancel: '取消',
    save: '保存',
    delete: '删除',
    edit: '编辑',
    add: '添加',
    close: '关闭',
    yes: '是',
    no: '否',
    or: '或',
    and: '和',
  },

  // Validation Errors
  validation: {
    required: '{{field}}是必填项',
    invalidFormat: '{{field}}格式不正确',
    alreadyExists: '{{field}}已存在',
  },

  // Error Messages
  errors: {
    operationFailed: '操作失败: {{message}}',
    createFailed: '创建失败: {{message}}',
    saveFailed: '保存失败: {{message}}',
    deleteFailed: '删除失败: {{message}}',
    copyFailed: '复制失败: {{message}}',
    networkError: '网络错误',
    unknownError: '未知错误',
  },
};

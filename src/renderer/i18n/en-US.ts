/**
 * English Translations for OA Proxy
 */

export const enUS = {
  // App
  app: {
    title: 'OA Proxy',
    protocolForwardingService: 'Protocol Forwarding Service',
    statusLoading: 'Loading status...',
  },

  // Header
  header: {
    serviceSwitch: 'Service Switch',
    addGroup: 'Add Group',
    settings: 'Settings',
    logs: 'Logs',
    backToService: 'Back to Service',
    serviceRunning: 'Running',
    serviceStopped: 'Stopped',
    start: 'Start',
    stop: 'Stop',
  },

  // Service Status
  service: {
    running: 'Running',
    stopped: 'Stopped',
    port: 'Port',
    host: 'Host',
    requests: 'Requests',
    errors: 'Errors',
    avgLatency: 'Avg Latency',
    start: 'Start',
    stop: 'Stop',
    statusText: '{{running}} | {{host}}:{{port}} | Requests {{requests}} | Errors {{errors}} | Avg Latency {{latency}}ms',
    stoppedText: 'Stopped | {{host}}:{{port}}',
  },

  // Service Page / Groups
  servicePage: {
    noGroupsHint: 'No groups yet. Click "Add Group" to create one.',
    createFirstGroup: 'Create First Group',
    noGroupSelected: 'Please select a group.',
    noRulesHint: 'This group has no rules yet. Click "Add Rule" to create one.',
    groupPath: 'Group ID',
    entryUrl: 'Entry URL',
    copyEntryUrl: 'Copy Entry URL',
    addRule: 'Add Rule',
    editGroup: 'Edit Group',
    deleteGroup: 'Delete Group',
    groupName: 'Group Name',
    model: 'Model Name',
    ruleName: 'Rule Name',
    ruleProtocol: 'Downstream Protocol',
    defaultModel: 'Default Model',
    token: 'Token',
    apiAddress: 'API Address',
    current: 'Currently Active',
    rulesCount: '{{count}} rules',
    modelsCount: '{{count}} models',
    saveRule: 'Save Rule',
    deleteRule: 'Delete',
  },

  // Rule Directions
  ruleDirection: {
    oc: 'OpenAI -> Anthropic',
    co: 'Anthropic -> OpenAI',
  },

  // Rule Protocol
  ruleProtocol: {
    openai: 'OpenAI',
    anthropic: 'Anthropic',
  },

  // Settings
  settings: {
    title: 'Service Settings',
    subtitle: 'Configure network binding, compatibility, startup behavior, and UI preferences.',
    networkSection: 'Network',
    behaviorSection: 'Behavior',
    interfaceSection: 'Interface',
    listenHost: 'Listen Host',
    hostHint: 'Use 0.0.0.0 to expose to LAN, or 127.0.0.1 for local only.',
    servicePort: 'Service Port',
    portHint: 'Allowed range: 1 - 65535.',
    strictMode: 'Strict Mode (fail fast on incompatible fields)',
    strictModeHint: 'When enabled, incompatible protocol fields fail immediately.',
    launchOnStartup: 'Launch at Startup',
    launchOnStartupHint: 'Auto-launch OA Proxy after system sign-in.',
    theme: 'Theme',
    themeHint: 'Controls app appearance in all pages.',
    themeLight: 'Light',
    themeDark: 'Dark',
    language: 'Language',
    languageHint: 'Controls interface language across the app.',
    languageEnglish: 'English',
    languageChinese: 'Simplified Chinese',
    previewTitle: 'Runtime Preview',
    previewAddress: 'Address',
    previewMode: 'Compatibility Mode',
    previewTheme: 'Theme',
    previewLanguage: 'Language',
    previewStartup: 'Launch at Startup',
    startupEnabled: 'Enabled',
    startupDisabled: 'Disabled',
    modeStrict: 'Strict',
    modeCompatible: 'Compatible',
    unsavedChanges: 'Unsaved changes',
    noChanges: 'No pending changes',
    save: 'Save',
    cancel: 'Cancel',
    saveSuccess: 'Settings saved',
    portError: 'Port must be an integer between 1 and 65535',
  },

  // Logs
  logs: {
    title: 'Request Log',
    recentLogs: 'Recent {{count}} entries',
    filteredLogs: 'Showing {{shown}} of {{total}} entries',
    refresh: 'Refresh',
    clear: 'Clear',
    filterByStatus: 'Filter by status',
    filterAll: 'All',
    resetFilter: 'Reset Filter',
    noLogs: 'No logs',
    noFilteredLogs: 'No logs for this status',
    refreshSuccess: 'Logs refreshed',
    clearSuccess: 'Logs cleared',
    refreshError: 'Failed to refresh logs',
    totalRequests: 'Total Requests',
    errorsCount: 'Errors',
    successRate: 'Success Rate',
    avgDuration: 'Avg Duration',
    // Log entries
    request: 'Request',
    status: 'Status',
    requestStatus: 'HTTP {{status}} | {{state}}',
    errorReason: ' | Reason: {{reason}}',
    requestBody: 'Request Body',
    group: 'Group',
    model: 'Model',
    duration: 'Duration',
    state: {
      ok: 'OK',
      error: 'Error',
      processing: 'Processing',
      rejected: 'Rejected',
    },
    forwardingTo: 'Forwarding to',
    notForwarding: 'Not forwarded (path/auth/rule validation failed)',
    separator: '----------------------------------------',
  },

  // Modals
  modal: {
    addGroupTitle: 'Add Group',
    groupNameLabel: 'Group Name',
    groupNamePlaceholder: 'e.g. claude',
    groupIdLabel: 'Group ID',
    groupIdPlaceholder: 'e.g. claude',
    groupIdHint: 'Request path is fixed to `/oc/{{id}}` and cannot be changed after creation.',
    create: 'Create',
    cancel: 'Cancel',
    save: 'Save',
  },

  // Delete Group Modal
  deleteGroupModal: {
    title: 'Confirm Delete Group',
    confirmText: 'This will delete group "{{name}}" (id: {{path}}). This action cannot be undone. Continue?',
    confirmDelete: 'Confirm Delete',
    cancel: 'Cancel',
  },

  // Delete Rule Modal
  deleteRuleModal: {
    title: 'Confirm Delete Rule',
    confirmText: 'This will delete rule "{{model}}". This action cannot be undone. Continue?',
    confirmDelete: 'Confirm Delete',
  },

  // Clear Logs Modal
  clearLogsModal: {
    title: 'Confirm Clear Logs',
    confirmText: 'This will clear {{count}} log entries. This action cannot be undone.',
    confirmClear: 'Clear Logs',
  },

  // Toast Messages
  toast: {
    serviceStarted: 'Service started',
    serviceStopped: 'Service stopped',
    restartComplete: 'Restart complete',
    groupCreated: 'Group created',
    groupUpdated: 'Group updated',
    groupDeleted: 'Group deleted',
    ruleCreated: 'Rule created',
    ruleUpdated: 'Rule updated',
    ruleSaved: 'Rule saved',
    ruleDeleted: 'Rule deleted',
    ruleSwitched: 'Active rule switched',
    entryUrlCopied: 'Entry URL copied',
    copyFailed: 'Copy failed',
    configSaved: 'Configuration saved',
    ruleNotFound: 'Rule not found',
    groupNotFound: 'Group not found',
  },

  // Rule Edit Page
  ruleEditPage: {
    title: 'Edit Rule',
    saveChanges: 'Save Changes',
  },

  // Rule Create Page
  ruleCreatePage: {
    title: 'Create New Rule',
    newRule: 'New Rule',
    createRule: 'Create Rule',
  },

  // Rule Form
  ruleForm: {
    sectionRouting: 'Routing',
    sectionModelSettings: 'Model Settings',
    sectionSecurity: 'Credentials & Upstream',
    protocolHint: 'Upstream protocol is detected by URL. This field configures downstream protocol.',
    ruleNamePlaceholder: 'e.g. anth-default',
    defaultModelPlaceholder: 'e.g. claude-3-7-sonnet',
    defaultModelHint: 'Required: used when upstream model is not listed in group models.',
    modelMappings: 'Model Mappings',
    noGroupModels: 'No group models yet. Add models in "Edit Group" first.',
    mappingPlaceholder: 'Target model name',
    tokenHint: 'Stored locally. Use an upstream token with only required scope.',
    endpointHint: 'Include protocol, e.g. https://api.anthropic.com',
    previewTitle: 'Rule Preview',
    previewPath: 'Entry Path',
    previewDirection: 'Direction',
    previewUpstream: 'Upstream',
  },

  // Group Edit Page
  groupEditPage: {
    title: 'Edit Group',
    sectionBasic: 'Basic',
    sectionModels: 'Group Models',
    groupIdImmutable: 'Group ID is used in request path and cannot be changed after creation.',
    noModels: 'No models yet. Add models to enable rule mappings.',
    newModelPlaceholder: 'e.g. a1',
  },

  // Common
  common: {
    loading: 'Loading...',
    error: 'Error',
    success: 'Success',
    failed: 'Failed',
    confirm: 'Confirm',
    cancel: 'Cancel',
    save: 'Save',
    delete: 'Delete',
    edit: 'Edit',
    add: 'Add',
    close: 'Close',
    yes: 'Yes',
    no: 'No',
    or: 'or',
    and: 'and',
  },

  // Validation Errors
  validation: {
    required: '{{field}} is required',
    invalidFormat: '{{field}} has invalid format',
    alreadyExists: '{{field}} already exists',
  },

  // Error Messages
  errors: {
    operationFailed: 'Operation failed: {{message}}',
    createFailed: 'Create failed: {{message}}',
    saveFailed: 'Save failed: {{message}}',
    deleteFailed: 'Delete failed: {{message}}',
    copyFailed: 'Copy failed: {{message}}',
    networkError: 'Network error',
    unknownError: 'Unknown error',
  },
};

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
    noGroupSelected: 'Please select a group.',
    noRulesHint: 'This group has no rules yet. Click "Add Rule" to create one.',
    groupPath: 'Group Path',
    entryUrl: 'Entry URL',
    copyEntryUrl: 'Copy Entry URL',
    addRule: 'Add Rule',
    deleteGroup: 'Delete Group',
    groupName: 'Group Name',
    model: 'Model Name',
    forwardDirection: 'Forward Direction',
    token: 'Token',
    apiAddress: 'API Address',
    current: 'Currently Active',
    saveRule: 'Save Rule',
    deleteRule: 'Delete',
  },

  // Rule Directions
  ruleDirection: {
    oc: 'OpenAI -> Anthropic',
    co: 'Anthropic -> OpenAI',
  },

  // Settings
  settings: {
    title: 'Service Settings',
    listenHost: 'Listen Host',
    servicePort: 'Service Port',
    strictMode: 'Strict Mode (fail fast on incompatible fields)',
    save: 'Save',
    cancel: 'Cancel',
    saveSuccess: 'Settings saved',
    portError: 'Port must be an integer between 1 and 65535',
  },

  // Logs
  logs: {
    title: 'Request Log',
    recentLogs: 'Recent {{count}} entries',
    refresh: 'Refresh',
    clear: 'Clear',
    noLogs: 'No logs',
    refreshSuccess: 'Logs refreshed',
    clearSuccess: 'Logs cleared',
    refreshError: 'Failed to refresh logs',
    // Log entries
    request: 'Request',
    status: 'Status',
    requestStatus: 'HTTP {{status}} | {{state}}',
    errorReason: ' | Reason: {{reason}}',
    requestBody: 'Request Body',
    forwardingTo: 'Forwarding to',
    notForwarding: 'Not forwarded (path/auth/rule validation failed)',
    separator: '----------------------------------------',
  },

  // Modals
  modal: {
    addGroupTitle: 'Add Group',
    groupNameLabel: 'Group Name',
    groupNamePlaceholder: 'e.g. claude',
    pathLabel: 'Forward Path',
    pathPlaceholder: 'e.g. claude',
    pathHint: 'Request path example: `/oc/{{path}}` (forwarding direction determined by active rule)',
    create: 'Create',
    cancel: 'Cancel',
    save: 'Save',
  },

  // Delete Group Modal
  deleteGroupModal: {
    title: 'Confirm Delete Group',
    confirmText: 'This will delete group "{{name}}" (path: {{path}}). This action cannot be undone. Continue?',
    confirmDelete: 'Confirm Delete',
    cancel: 'Cancel',
  },

  // Toast Messages
  toast: {
    serviceStarted: 'Service started',
    serviceStopped: 'Service stopped',
    restartComplete: 'Restart complete',
    groupCreated: 'Group created',
    groupDeleted: 'Group deleted',
    ruleCreated: 'Rule created',
    ruleSaved: 'Rule saved',
    ruleDeleted: 'Rule deleted',
    ruleSwitched: 'Active rule switched',
    entryUrlCopied: 'Entry URL copied',
    copyFailed: 'Copy failed',
    configSaved: 'Configuration saved',
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

const DEFAULT_CONFIG = {
  server: {
    host: "0.0.0.0",
    port: 8899,
    authEnabled: false,
    localBearerToken: ""
  },
  compat: {
    strictMode: true
  },
  logging: {
    level: "info",
    captureBody: true,
    redactRules: [
      "authorization",
      "x-api-key",
      "api-key",
      "api_key",
      "token",
      "password"
    ]
  },
  groups: []
};

function getDefaultConfig() {
  return JSON.parse(JSON.stringify(DEFAULT_CONFIG));
}

module.exports = {
  getDefaultConfig
};

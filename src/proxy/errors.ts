// @ts-nocheck
function toProxyError(err, traceId, protocol) {
  const statusCode = err.statusCode || 500;
  return {
    statusCode,
    body: {
      error: {
        code: err.code || "proxy_error",
        message: err.message || "Unknown proxy error",
        upstreamStatus: err.upstreamStatus,
        protocol,
        traceId
      }
    }
  };
}

module.exports = {
  toProxyError
};

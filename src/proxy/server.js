const http = require("node:http");
const { randomUUID } = require("node:crypto");
const { redactPayload } = require("./redact");
const { toProxyError } = require("./errors");
const {
  normalizeOpenAIRequest,
  mapOpenAIToAnthropicRequest,
  mapAnthropicToOpenAIResponse
} = require("./mappers/openaiToAnthropic");
const {
  mapAnthropicToOpenAIRequest,
  mapOpenAIToAnthropicResponse
} = require("./mappers/anthropicToOpenai");
const { createSSEParser, beginSSE, writeSSE } = require("./sse");

async function readRequestBody(req) {
  const chunks = [];
  for await (const chunk of req) {
    chunks.push(chunk);
  }
  if (chunks.length === 0) return {};
  const raw = Buffer.concat(chunks).toString("utf-8");
  if (!raw.trim()) return {};
  try {
    return JSON.parse(raw);
  } catch {
    const err = new Error("Request body must be valid JSON");
    err.statusCode = 400;
    throw err;
  }
}

function sendJson(res, statusCode, payload, extraHeaders = {}) {
  res.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
    ...extraHeaders
  });
  res.end(JSON.stringify(payload));
}

function isSSEResponse(headers = {}) {
  const ct = String(headers.get ? headers.get("content-type") : headers["content-type"] || "");
  return ct.toLowerCase().includes("text/event-stream");
}

function normalizeHeaders(headers) {
  const out = {};
  for (const [k, v] of Object.entries(headers || {})) {
    out[String(k).toLowerCase()] = Array.isArray(v) ? v.join(",") : String(v);
  }
  return out;
}

function findGroupAndRule(config, groupPath) {
  const group = (config.groups || []).find((g) => g.path === groupPath);
  if (!group) {
    const err = new Error(`Group not found for path: ${groupPath}`);
    err.statusCode = 404;
    throw err;
  }

  if (!group.activeRuleId) {
    const err = new Error(`Group ${group.name} has no active rule`);
    err.statusCode = 409;
    throw err;
  }

  const rule = (group.rules || []).find((r) => r.id === group.activeRuleId);
  if (!rule) {
    const err = new Error(`Active rule ${group.activeRuleId} is missing in group ${group.name}`);
    err.statusCode = 409;
    throw err;
  }

  return {
    group,
    rule
  };
}

function assertRuleReady(rule) {
  if (!rule.model || !String(rule.model).trim()) {
    const err = new Error("Active rule model is empty");
    err.statusCode = 409;
    throw err;
  }
  if (!rule.token || !String(rule.token).trim()) {
    const err = new Error("Active rule token is empty");
    err.statusCode = 409;
    throw err;
  }
  if (!rule.apiAddress || !String(rule.apiAddress).trim()) {
    const err = new Error("Active rule apiAddress is empty");
    err.statusCode = 409;
    throw err;
  }
}

function resolveUpstreamUrl(apiAddress, defaultPath) {
  let url;
  try {
    url = new URL(apiAddress);
  } catch {
    const err = new Error("rule.apiAddress must be a valid absolute URL");
    err.statusCode = 400;
    throw err;
  }

  const basePath = url.pathname && url.pathname !== "/" ? url.pathname.replace(/\/+$/, "") : "";
  const endpointPath = String(defaultPath || "/");

  if (!basePath) {
    url.pathname = endpointPath;
    return url.toString();
  }

  if (endpointPath === basePath || endpointPath.startsWith(`${basePath}/`)) {
    url.pathname = endpointPath;
    return url.toString();
  }

  url.pathname = `${basePath}${endpointPath.startsWith("/") ? endpointPath : `/${endpointPath}`}`;
  return url.toString();
}

function buildRuleHeaders(direction, rule) {
  const headers = {
    "content-type": "application/json"
  };

  if (direction === "oc") {
    headers["x-api-key"] = rule.token;
    headers["anthropic-version"] = "2023-06-01";
  } else {
    headers.authorization = `Bearer ${rule.token}`;
  }

  return headers;
}

function ensureModel(body, model) {
  if (!body || typeof body !== "object") {
    return { model };
  }
  return {
    ...body,
    model
  };
}

function toRedacted(payload, config) {
  if (!config.logging.captureBody) {
    return { omitted: true };
  }
  return redactPayload(payload, config.logging.redactRules);
}

function toRedactedHeaders(payload, config) {
  return redactPayload(payload, config.logging.redactRules);
}

class ProxyServer {
  constructor(configStore, logStore) {
    this.configStore = configStore;
    this.logStore = logStore;
    this.server = null;
    this.address = null;
    this.metrics = {
      requests: 0,
      streamRequests: 0,
      errors: 0,
      avgLatencyMs: 0,
      uptimeStartedAt: null
    };
  }

  isRunning() {
    return !!this.server;
  }

  getStatus() {
    return {
      running: this.isRunning(),
      address: this.address,
      metrics: { ...this.metrics }
    };
  }

  async start() {
    if (this.server) {
      return this.getStatus();
    }

    const config = this.configStore.get();
    this.server = http.createServer((req, res) => {
      const traceId = randomUUID();
      this.handleRequest(req, res, traceId).catch((err) => {
        const mapped = toProxyError(err, traceId, "proxy");
        this.metrics.errors += 1;

        if (!err.__logged) {
          this.logStore.append({
            traceId,
            phase: "request_chain",
            status: "error",
            method: req.method || "GET",
            requestPath: req.url?.split("?")[0] || "/",
            requestAddress: `${req.method || "GET"} ${req.url || "/"}`,
            forwardingAddress: "",
            requestBody: null,
            forwardRequestBody: null,
            responseBody: null,
            httpStatus: mapped.statusCode,
            upstreamStatus: err.upstreamStatus || null,
            durationMs: 0,
            error: {
              message: err.message || "Unhandled proxy error",
              code: err.code || "proxy_error"
            }
          });
        }

        sendJson(res, mapped.statusCode, mapped.body, { "x-trace-id": traceId });
      });
    });

    await new Promise((resolve, reject) => {
      this.server.once("error", reject);
      this.server.listen(config.server.port, config.server.host, () => {
        this.server.off("error", reject);
        resolve();
      });
    });

    this.address = `http://${config.server.host}:${config.server.port}`;
    this.metrics.uptimeStartedAt = new Date().toISOString();
    return this.getStatus();
  }

  async stop() {
    if (!this.server) {
      return this.getStatus();
    }

    await new Promise((resolve, reject) => {
      this.server.close((err) => (err ? reject(err) : resolve()));
    });

    this.server = null;
    this.address = null;
    this.metrics.uptimeStartedAt = null;
    return this.getStatus();
  }

  createRequestChain(req, path, method, traceId, config) {
    const headers = normalizeHeaders(req.headers);
    const host = headers.host || `${config.server.host}:${config.server.port}`;

    return {
      traceId,
      phase: "request_chain",
      status: "processing",
      method,
      requestPath: path,
      requestAddress: `${method} http://${host}${path}`,
      clientAddress: req.socket?.remoteAddress || "",
      groupPath: null,
      groupName: null,
      ruleId: null,
      direction: null,
      model: null,
      forwardingAddress: null,
      requestHeaders: toRedactedHeaders(headers, config),
      requestBody: null,
      forwardRequestBody: null,
      responseBody: null,
      httpStatus: null,
      upstreamStatus: null,
      durationMs: 0,
      error: null
    };
  }

  finalizeRequestChain(chain, started, patch) {
    const merged = {
      ...chain,
      ...patch,
      durationMs: Date.now() - started
    };
    this.logStore.append(merged);
  }

  async handleRequest(req, res, traceId) {
    const started = Date.now();
    const config = this.configStore.get();
    const path = req.url?.split("?")[0] || "/";
    const method = req.method || "GET";
    const chain = this.createRequestChain(req, path, method, traceId, config);

    try {
      if (method === "GET" && path === "/healthz") {
        sendJson(res, 200, { ok: true, running: true }, { "x-trace-id": traceId });
        this.finalizeRequestChain(chain, started, {
          status: "ok",
          httpStatus: 200,
          responseBody: { ok: true, running: true }
        });
        return;
      }

      if (method === "GET" && path === "/metrics-lite") {
        sendJson(res, 200, this.metrics, { "x-trace-id": traceId });
        this.finalizeRequestChain(chain, started, {
          status: "ok",
          httpStatus: 200,
          responseBody: this.metrics
        });
        return;
      }

      if (config.server.authEnabled) {
        const auth = req.headers.authorization || "";
        const expected = `Bearer ${config.server.localBearerToken}`;
        if (auth !== expected) {
          const payload = { error: { code: "unauthorized", message: "Missing or invalid local bearer token" } };
          sendJson(res, 401, payload, { "x-trace-id": traceId });
          this.finalizeRequestChain(chain, started, {
            status: "rejected",
            httpStatus: 401,
            responseBody: payload,
            error: { message: "invalid_local_token", code: "unauthorized" }
          });
          return;
        }
      }

      const matched = path.match(/^\/oc\/([a-zA-Z0-9_-]+)(?:\/.*)?$/);
      if (method !== "POST" || !matched) {
        const payload = { error: { code: "not_found", message: "Use POST /oc/:groupPath (optional SDK suffix is supported)" } };
        sendJson(res, 404, payload, { "x-trace-id": traceId });
        this.finalizeRequestChain(chain, started, {
          status: "rejected",
          httpStatus: 404,
          responseBody: payload,
          error: { message: "invalid_path", code: "not_found" }
        });
        return;
      }

      const groupPath = matched[1];
      chain.groupPath = groupPath;

      const requestBody = await readRequestBody(req);
      chain.requestBody = toRedacted(requestBody, config);

      const { group, rule } = findGroupAndRule(config, groupPath);
      assertRuleReady(rule);

      chain.groupName = group.name;
      chain.ruleId = rule.id;
      chain.model = rule.model;
      chain.direction = rule.direction;

      const direction = rule.direction;
      if (direction !== "oc" && direction !== "co") {
        const err = new Error(`Unsupported rule direction: ${direction}`);
        err.statusCode = 409;
        throw err;
      }

      const upstreamPath = direction === "oc" ? "/v1/messages" : "/v1/chat/completions";
      const upstreamUrl = resolveUpstreamUrl(rule.apiAddress, upstreamPath);
      chain.forwardingAddress = upstreamUrl;

      let upstreamBody;
      let stream = false;
      const requestedModel = requestBody.model || rule.model;

      if (direction === "oc") {
        const normalized = requestBody.input != null && requestBody.messages == null
          ? normalizeOpenAIRequest("/v1/responses", requestBody)
          : requestBody;

        upstreamBody = mapOpenAIToAnthropicRequest(ensureModel(normalized, rule.model), {
          strictMode: config.compat.strictMode,
          targetModel: rule.model
        });
        stream = !!upstreamBody.stream;
      } else {
        upstreamBody = mapAnthropicToOpenAIRequest(ensureModel(requestBody, rule.model), {
          strictMode: config.compat.strictMode,
          targetModel: rule.model
        });
        stream = !!upstreamBody.stream;
      }
      chain.forwardRequestBody = toRedacted(upstreamBody, config);

      this.metrics.requests += 1;
      if (stream) this.metrics.streamRequests += 1;

      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 60000);

      let upstreamResponse;
      try {
        upstreamResponse = await fetch(upstreamUrl, {
          method: "POST",
          headers: buildRuleHeaders(direction, rule),
          body: JSON.stringify(upstreamBody),
          signal: controller.signal
        });
      } catch (err) {
        clearTimeout(timer);
        err.message = `Upstream request failed: ${err.message}`;
        err.statusCode = 502;
        throw err;
      } finally {
        clearTimeout(timer);
      }

      chain.upstreamStatus = upstreamResponse.status;

      if (stream && isSSEResponse(upstreamResponse.headers)) {
        if (!upstreamResponse.ok) {
          const text = await upstreamResponse.text();
          const err = new Error(`Upstream stream failed: ${text}`);
          err.statusCode = 502;
          err.upstreamStatus = upstreamResponse.status;
          throw err;
        }

        if (direction === "oc") {
          await this.bridgeAnthropicToOpenAI(upstreamResponse, res, traceId, requestedModel);
        } else {
          await this.bridgeOpenAIToAnthropic(upstreamResponse, res, traceId, requestedModel);
        }

        this.updateLatency(Date.now() - started);
        this.finalizeRequestChain(chain, started, {
          status: "ok",
          httpStatus: 200,
          responseBody: { stream: true }
        });
        return;
      }

      const rawText = await upstreamResponse.text();
      let upstreamJson;
      try {
        upstreamJson = rawText ? JSON.parse(rawText) : {};
      } catch {
        const err = new Error(`Upstream returned non-JSON response: ${rawText.slice(0, 200)}`);
        err.statusCode = 502;
        err.upstreamStatus = upstreamResponse.status;
        throw err;
      }

      if (!upstreamResponse.ok) {
        const err = new Error(upstreamJson.error?.message || "Upstream returned error");
        err.statusCode = 502;
        err.upstreamStatus = upstreamResponse.status;
        throw err;
      }

      let outputBody = upstreamJson;
      if (direction === "oc") {
        outputBody = mapAnthropicToOpenAIResponse(upstreamJson, { requestModel: requestedModel });
      } else {
        outputBody = mapOpenAIToAnthropicResponse(upstreamJson, { requestModel: requestedModel });
      }

      sendJson(res, 200, outputBody, { "x-trace-id": traceId });
      this.updateLatency(Date.now() - started);

      this.finalizeRequestChain(chain, started, {
        status: "ok",
        httpStatus: 200,
        responseBody: toRedacted(outputBody, config)
      });
    } catch (err) {
      this.finalizeRequestChain(chain, started, {
        status: "error",
        httpStatus: err.statusCode || 500,
        upstreamStatus: err.upstreamStatus || null,
        responseBody: null,
        error: {
          message: err.message || "proxy_error",
          code: err.code || "proxy_error"
        }
      });
      err.__logged = true;
      throw err;
    }
  }

  updateLatency(ms) {
    const n = this.metrics.requests;
    if (n <= 1) {
      this.metrics.avgLatencyMs = ms;
      return;
    }
    this.metrics.avgLatencyMs = Math.round(((this.metrics.avgLatencyMs * (n - 1)) + ms) / n);
  }

  async bridgeAnthropicToOpenAI(upstreamResponse, res, traceId, requestModel) {
    beginSSE(res, { "x-trace-id": traceId });

    const reader = upstreamResponse.body.getReader();
    const decoder = new TextDecoder();

    let messageId = `chatcmpl_${randomUUID().replace(/-/g, "")}`;
    const created = Math.floor(Date.now() / 1000);
    let emittedRole = false;

    const parser = createSSEParser(({ event, data }) => {
      if (data === "[DONE]") {
        res.write("data: [DONE]\n\n");
        return;
      }

      let payload;
      try {
        payload = JSON.parse(data);
      } catch {
        return;
      }

      if (event === "message_start" && payload.message?.id) {
        messageId = payload.message.id;
      }

      if (!emittedRole) {
        emittedRole = true;
        res.write(`data: ${JSON.stringify({
          id: messageId,
          object: "chat.completion.chunk",
          created,
          model: requestModel,
          choices: [{ index: 0, delta: { role: "assistant" }, finish_reason: null }]
        })}\n\n`);
      }

      if (event === "content_block_delta" && payload.delta?.text) {
        res.write(`data: ${JSON.stringify({
          id: messageId,
          object: "chat.completion.chunk",
          created,
          model: requestModel,
          choices: [{ index: 0, delta: { content: payload.delta.text }, finish_reason: null }]
        })}\n\n`);
      }

      if (event === "message_stop") {
        res.write(`data: ${JSON.stringify({
          id: messageId,
          object: "chat.completion.chunk",
          created,
          model: requestModel,
          choices: [{ index: 0, delta: {}, finish_reason: "stop" }]
        })}\n\n`);
        res.write("data: [DONE]\n\n");
      }
    });

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.write(decoder.decode(value, { stream: true }));
    }
    parser.end();
    res.end();
  }

  async bridgeOpenAIToAnthropic(upstreamResponse, res, traceId, requestModel) {
    beginSSE(res, { "x-trace-id": traceId });

    const msgId = `msg_${randomUUID().replace(/-/g, "")}`;
    writeSSE(res, {
      event: "message_start",
      data: JSON.stringify({
        type: "message_start",
        message: {
          id: msgId,
          type: "message",
          role: "assistant",
          model: requestModel,
          content: []
        }
      })
    });

    const reader = upstreamResponse.body.getReader();
    const decoder = new TextDecoder();
    const parser = createSSEParser(({ data }) => {
      if (data === "[DONE]") {
        writeSSE(res, { event: "message_stop", data: JSON.stringify({ type: "message_stop" }) });
        return;
      }

      let payload;
      try {
        payload = JSON.parse(data);
      } catch {
        return;
      }

      const choice = payload.choices?.[0];
      const delta = choice?.delta || {};

      if (typeof delta.content === "string" && delta.content.length > 0) {
        writeSSE(res, {
          event: "content_block_delta",
          data: JSON.stringify({
            type: "content_block_delta",
            index: 0,
            delta: {
              type: "text_delta",
              text: delta.content
            }
          })
        });
      }

      if (choice?.finish_reason) {
        writeSSE(res, { event: "message_stop", data: JSON.stringify({ type: "message_stop" }) });
      }
    });

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.write(decoder.decode(value, { stream: true }));
    }
    parser.end();
    res.end();
  }
}

module.exports = {
  ProxyServer
};

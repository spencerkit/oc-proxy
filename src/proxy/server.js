const http = require("node:http");
const { randomUUID } = require("node:crypto");
const { redactPayload } = require("./redact");
const { toProxyError } = require("./errors");
const {
  normalizeOpenAIRequest,
  mapOpenAIToAnthropicRequest,
  mapAnthropicToOpenAIResponse,
  mapOpenAIChatToResponses
} = require("./mappers/openaiToAnthropic");
const {
  mapAnthropicToOpenAIRequest,
  mapOpenAIToAnthropicResponse
} = require("./mappers/anthropicToOpenai");
const { createSSEParser, beginSSE, writeSSE } = require("./sse");

const MAX_REQUEST_BODY_BYTES = 10 * 1024 * 1024;

async function readRequestBody(req) {
  const chunks = [];
  let totalBytes = 0;
  for await (const chunk of req) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    totalBytes += buffer.length;
    if (totalBytes > MAX_REQUEST_BODY_BYTES) {
      const err = new Error(`Request body too large (max ${MAX_REQUEST_BODY_BYTES} bytes)`);
      err.statusCode = 413;
      throw err;
    }
    chunks.push(buffer);
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

function toPlainHeaders(headers) {
  const out = {};

  if (!headers) {
    return out;
  }

  if (typeof headers.forEach === "function") {
    headers.forEach((value, key) => {
      out[String(key).toLowerCase()] = String(value);
    });
    return out;
  }

  for (const [k, v] of Object.entries(headers)) {
    out[String(k).toLowerCase()] = Array.isArray(v) ? v.join(",") : String(v);
  }

  return out;
}

function responseHeadersForJson(traceId) {
  return {
    "content-type": "application/json; charset=utf-8",
    "x-trace-id": traceId
  };
}

function responseHeadersForSSE(traceId) {
  return {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache, no-transform",
    connection: "keep-alive",
    "x-accel-buffering": "no",
    "x-trace-id": traceId
  };
}

function findGroupAndRule(config, groupId) {
  const groups = config.groups || [];
  const group = groups.find((g) => g.id === groupId);
  if (!group) {
    const err = new Error(`Group not found for id: ${groupId}`);
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
  if (!rule.name || !String(rule.name).trim()) {
    const err = new Error("Active rule name is empty");
    err.statusCode = 409;
    throw err;
  }
  if (!rule.defaultModel || !String(rule.defaultModel).trim()) {
    const err = new Error("Active rule defaultModel is empty");
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

function buildRuleHeaders(protocol, rule) {
  const headers = {
    "content-type": "application/json"
  };

  if (protocol === "anthropic") {
    headers["x-api-key"] = rule.token;
    headers["anthropic-version"] = "2023-06-01";
  } else {
    headers.authorization = `Bearer ${rule.token}`;
  }

  return headers;
}

function parseProxyRequestPath(path) {
  const matched = path.match(/^\/oc\/([a-zA-Z0-9_-]+)(\/.*)?$/);
  if (!matched) return null;
  return {
    groupId: matched[1],
    suffixPath: matched[2] || ""
  };
}

function detectEntryProtocol(suffixPath) {
  const normalized = suffixPath && suffixPath !== "/" ? suffixPath : "/chat/completions";
  const cleanPath = (normalized.startsWith("/") ? normalized : `/${normalized}`).replace(/\/+$/, "") || "/";

  const isAnthropic = cleanPath === "/messages" || cleanPath === "/v1/messages";
  if (isAnthropic) {
    return {
      protocol: "anthropic",
      endpoint: "messages"
    };
  }

  const isOpenAIChat = cleanPath === "/chat/completions" || cleanPath === "/v1/chat/completions";
  if (isOpenAIChat) {
    return {
      protocol: "openai",
      endpoint: "chat_completions"
    };
  }

  const isOpenAIResponses = cleanPath === "/responses" || cleanPath === "/v1/responses";
  if (isOpenAIResponses) {
    return {
      protocol: "openai",
      endpoint: "responses"
    };
  }

  return null;
}

function resolveTargetModel(rule, group, requestBody) {
  const requestedModel = requestBody?.model;
  const normalizedRequestedModel = typeof requestedModel === "string" ? requestedModel.trim() : "";
  const groupModels = Array.isArray(group.models) ? group.models : [];
  const mappings = rule.modelMappings && typeof rule.modelMappings === "object" ? rule.modelMappings : {};

  if (!normalizedRequestedModel) {
    return rule.defaultModel;
  }

  if (groupModels.includes(normalizedRequestedModel)) {
    const mapped = mappings[normalizedRequestedModel];
    if (typeof mapped === "string" && mapped.trim()) {
      return mapped.trim();
    }
    return normalizedRequestedModel;
  }

  return rule.defaultModel;
}

function resolveUpstreamPath(targetProtocol, entryEndpoint) {
  if (targetProtocol === "anthropic") {
    return "/v1/messages";
  }
  if (entryEndpoint === "responses") {
    return "/v1/responses";
  }
  return "/v1/chat/completions";
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

function mapAnthropicStopReason(stopReason) {
  if (stopReason === "tool_use") return "tool_calls";
  if (stopReason === "max_tokens") return "length";
  return "stop";
}

function toStatusCode(value, fallback = 502) {
  const asNumber = Number(value);
  if (Number.isInteger(asNumber) && asNumber >= 100 && asNumber <= 599) {
    return asNumber;
  }
  return fallback;
}

function buildUpstreamError(upstreamStatus, message, code = "upstream_error") {
  const err = new Error(message || "Upstream returned error");
  err.code = code;
  err.statusCode = toStatusCode(upstreamStatus, 502);
  err.upstreamStatus = upstreamStatus;
  return err;
}

function toAnthropicUsage(usage) {
  const inputTokens = Number.isFinite(usage?.prompt_tokens) ? usage.prompt_tokens : 0;
  const outputTokens = Number.isFinite(usage?.completion_tokens) ? usage.completion_tokens : 0;
  return {
    input_tokens: inputTokens,
    output_tokens: outputTokens
  };
}

function toAnthropicStopReason(finishReason) {
  if (finishReason === "tool_calls") return "tool_use";
  if (finishReason === "length") return "max_tokens";
  return "end_turn";
}

function toOpenAIResponsesUsage(anthropicUsage) {
  const inputTokens = Number.isFinite(anthropicUsage?.input_tokens) ? anthropicUsage.input_tokens : 0;
  const outputTokens = Number.isFinite(anthropicUsage?.output_tokens) ? anthropicUsage.output_tokens : 0;
  return {
    input_tokens: inputTokens,
    output_tokens: outputTokens,
    total_tokens: inputTokens + outputTokens
  };
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
            forwardRequestHeaders: null,
            upstreamResponseHeaders: null,
            responseHeaders: responseHeadersForJson(traceId),
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
      entryProtocol: null,
      downstreamProtocol: null,
      model: null,
      forwardedModel: null,
      forwardingAddress: null,
      requestHeaders: toRedactedHeaders(headers, config),
      forwardRequestHeaders: null,
      upstreamResponseHeaders: null,
      responseHeaders: null,
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
          responseHeaders: toRedactedHeaders(responseHeadersForJson(traceId), config),
          responseBody: { ok: true, running: true }
        });
        return;
      }

      if (method === "GET" && path === "/metrics-lite") {
        sendJson(res, 200, this.metrics, { "x-trace-id": traceId });
        this.finalizeRequestChain(chain, started, {
          status: "ok",
          httpStatus: 200,
          responseHeaders: toRedactedHeaders(responseHeadersForJson(traceId), config),
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
            responseHeaders: toRedactedHeaders(responseHeadersForJson(traceId), config),
            responseBody: payload,
            error: { message: "invalid_local_token", code: "unauthorized" }
          });
          return;
        }
      }

      const parsedPath = parseProxyRequestPath(path);
      if (method !== "POST" || !parsedPath) {
        const payload = { error: { code: "not_found", message: "Use POST /oc/:groupId/:endpoint (messages/chat/completions/responses)" } };
        sendJson(res, 404, payload, { "x-trace-id": traceId });
        this.finalizeRequestChain(chain, started, {
          status: "rejected",
          httpStatus: 404,
          responseHeaders: toRedactedHeaders(responseHeadersForJson(traceId), config),
          responseBody: payload,
          error: { message: "invalid_path", code: "not_found" }
        });
        return;
      }

      const { groupId, suffixPath } = parsedPath;
      chain.groupPath = groupId;

      const requestBody = await readRequestBody(req);
      chain.requestBody = toRedacted(requestBody, config);

      const entry = detectEntryProtocol(suffixPath);
      if (!entry) {
        const err = new Error(`Unsupported entry path: /oc/${groupId}${suffixPath || ""}`);
        err.statusCode = 404;
        throw err;
      }

      const { group, rule } = findGroupAndRule(config, groupId);
      assertRuleReady(rule);

      chain.groupName = group.name;
      chain.ruleId = rule.id;
      chain.model = rule.name;
      chain.entryProtocol = entry.protocol;
      chain.downstreamProtocol = rule.protocol;
      chain.direction = entry.protocol === "openai" && rule.protocol === "anthropic"
        ? "oc"
        : (entry.protocol === "anthropic" && rule.protocol === "openai" ? "co" : null);

      const downstreamProtocol = rule.protocol;
      if (downstreamProtocol !== "openai" && downstreamProtocol !== "anthropic") {
        const err = new Error(`Unsupported rule protocol: ${downstreamProtocol}`);
        err.statusCode = 409;
        throw err;
      }

      const targetModel = resolveTargetModel(rule, group, requestBody);
      const requestedModel = typeof requestBody?.model === "string" ? requestBody.model : rule.defaultModel;
      chain.model = requestedModel;
      chain.forwardedModel = targetModel;
      const upstreamPath = resolveUpstreamPath(downstreamProtocol, entry.endpoint);
      const upstreamUrl = resolveUpstreamUrl(rule.apiAddress, upstreamPath);
      chain.forwardingAddress = upstreamUrl;

      let upstreamBody;
      let stream = false;

      if (entry.protocol === "openai" && downstreamProtocol === "anthropic") {
        const normalized = entry.endpoint === "responses"
          ? normalizeOpenAIRequest("/v1/responses", requestBody)
          : requestBody;
        upstreamBody = mapOpenAIToAnthropicRequest(ensureModel(normalized, targetModel), {
          strictMode: config.compat.strictMode,
          targetModel
        });
      } else if (entry.protocol === "anthropic" && downstreamProtocol === "openai") {
        upstreamBody = mapAnthropicToOpenAIRequest(ensureModel(requestBody, targetModel), {
          strictMode: config.compat.strictMode,
          targetModel
        });
      } else {
        upstreamBody = ensureModel(requestBody, targetModel);
      }

      stream = !!upstreamBody.stream;
      chain.forwardRequestBody = toRedacted(upstreamBody, config);
      const upstreamRequestHeaders = buildRuleHeaders(downstreamProtocol, rule);
      chain.forwardRequestHeaders = toRedactedHeaders(upstreamRequestHeaders, config);

      this.metrics.requests += 1;
      if (stream) this.metrics.streamRequests += 1;

      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 60000);

      let upstreamResponse;
      try {
        upstreamResponse = await fetch(upstreamUrl, {
          method: "POST",
          headers: upstreamRequestHeaders,
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
      chain.upstreamResponseHeaders = toRedactedHeaders(toPlainHeaders(upstreamResponse.headers), config);

      if (stream && isSSEResponse(upstreamResponse.headers)) {
        if (!upstreamResponse.ok) {
          const text = await upstreamResponse.text();
          throw buildUpstreamError(
            upstreamResponse.status,
            `Upstream stream failed: ${text || `HTTP ${upstreamResponse.status}`}`
          );
        }

        if (entry.protocol === "openai" && downstreamProtocol === "anthropic") {
          if (entry.endpoint === "responses") {
            await this.bridgeAnthropicToOpenAIResponses(upstreamResponse, res, traceId, requestedModel);
          } else {
            await this.bridgeAnthropicToOpenAI(upstreamResponse, res, traceId, requestedModel);
          }
        } else if (entry.protocol === "anthropic" && downstreamProtocol === "openai") {
          await this.bridgeOpenAIToAnthropic(upstreamResponse, res, traceId, requestedModel);
        } else {
          await this.pipeSSE(upstreamResponse, res, traceId);
        }

        this.updateLatency(Date.now() - started);
        this.finalizeRequestChain(chain, started, {
          status: "ok",
          httpStatus: 200,
          responseHeaders: toRedactedHeaders(responseHeadersForSSE(traceId), config),
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
        throw buildUpstreamError(
          upstreamResponse.status,
          upstreamJson.error?.message || `Upstream returned HTTP ${upstreamResponse.status}`
        );
      }

      let outputBody = upstreamJson;
      if (entry.protocol === "openai" && downstreamProtocol === "anthropic") {
        const chatBody = mapAnthropicToOpenAIResponse(upstreamJson, { requestModel: requestedModel });
        outputBody = entry.endpoint === "responses" ? mapOpenAIChatToResponses(chatBody) : chatBody;
      } else if (entry.protocol === "anthropic" && downstreamProtocol === "openai") {
        outputBody = mapOpenAIToAnthropicResponse(upstreamJson, { requestModel: requestedModel });
      }

      sendJson(res, 200, outputBody, { "x-trace-id": traceId });
      this.updateLatency(Date.now() - started);

      this.finalizeRequestChain(chain, started, {
        status: "ok",
        httpStatus: 200,
        responseHeaders: toRedactedHeaders(responseHeadersForJson(traceId), config),
        responseBody: toRedacted(outputBody, config)
      });
    } catch (err) {
      this.finalizeRequestChain(chain, started, {
        status: "error",
        httpStatus: err.statusCode || 500,
        upstreamStatus: err.upstreamStatus || null,
        responseHeaders: toRedactedHeaders(responseHeadersForJson(traceId), config),
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

  async pipeSSE(upstreamResponse, res, traceId) {
    beginSSE(res, { "x-trace-id": traceId });
    const reader = upstreamResponse.body.getReader();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(Buffer.from(value));
    }
    res.end();
  }

  async bridgeAnthropicToOpenAI(upstreamResponse, res, traceId, requestModel) {
    beginSSE(res, { "x-trace-id": traceId });

    const reader = upstreamResponse.body.getReader();
    const decoder = new TextDecoder();

    let messageId = `chatcmpl_${randomUUID().replace(/-/g, "")}`;
    const created = Math.floor(Date.now() / 1000);
    let emittedRole = false;
    let doneSent = false;
    let finishReason = "stop";
    const toolIndexesByContentIndex = new Map();
    let nextToolIndex = 0;

    const ensureRole = () => {
      if (emittedRole) return;
      emittedRole = true;
      res.write(`data: ${JSON.stringify({
        id: messageId,
        object: "chat.completion.chunk",
        created,
        model: requestModel,
        choices: [{ index: 0, delta: { role: "assistant" }, finish_reason: null }]
      })}\n\n`);
    };

    const emitChunk = (delta, chunkFinishReason = null) => {
      ensureRole();
      res.write(`data: ${JSON.stringify({
        id: messageId,
        object: "chat.completion.chunk",
        created,
        model: requestModel,
        choices: [{ index: 0, delta, finish_reason: chunkFinishReason }]
      })}\n\n`);
    };

    const emitDone = () => {
      if (doneSent) return;
      doneSent = true;
      emitChunk({}, finishReason);
      res.write("data: [DONE]\n\n");
    };

    const parser = createSSEParser(({ event, data }) => {
      if (data === "[DONE]") {
        emitDone();
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

      if (event === "message_delta" && payload.delta?.stop_reason) {
        finishReason = mapAnthropicStopReason(payload.delta.stop_reason);
        return;
      }

      if (event === "content_block_delta" && payload.delta?.text) {
        emitChunk({ content: payload.delta.text });
        return;
      }

      if (event === "content_block_start" && payload.content_block?.type === "tool_use") {
        const contentIndex = Number.isInteger(payload.index) ? payload.index : nextToolIndex;
        const existingToolIndex = toolIndexesByContentIndex.get(contentIndex);
        const toolIndex = existingToolIndex == null ? nextToolIndex++ : existingToolIndex;
        toolIndexesByContentIndex.set(contentIndex, toolIndex);
        finishReason = "tool_calls";
        emitChunk({
          tool_calls: [{
            index: toolIndex,
            id: payload.content_block.id || `call_${randomUUID().replace(/-/g, "")}`,
            type: "function",
            function: {
              name: payload.content_block.name || "tool",
              arguments: ""
            }
          }]
        });
        return;
      }

      if (
        event === "content_block_delta"
        && payload.delta?.type === "input_json_delta"
        && typeof payload.delta.partial_json === "string"
      ) {
        const contentIndex = Number.isInteger(payload.index) ? payload.index : 0;
        const existingToolIndex = toolIndexesByContentIndex.get(contentIndex);
        const toolIndex = existingToolIndex == null ? nextToolIndex++ : existingToolIndex;
        toolIndexesByContentIndex.set(contentIndex, toolIndex);
        finishReason = "tool_calls";
        emitChunk({
          tool_calls: [{
            index: toolIndex,
            type: "function",
            function: {
              arguments: payload.delta.partial_json
            }
          }]
        });
        return;
      }

      if (event === "message_stop") {
        emitDone();
      }
    });

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.write(decoder.decode(value, { stream: true }));
    }
    parser.end();
    emitDone();
    res.end();
  }

  async bridgeAnthropicToOpenAIResponses(upstreamResponse, res, traceId, requestModel) {
    beginSSE(res, { "x-trace-id": traceId });

    const reader = upstreamResponse.body.getReader();
    const decoder = new TextDecoder();
    const responseId = `resp_${randomUUID().replace(/-/g, "")}`;
    const createdAt = Math.floor(Date.now() / 1000);
    const outputItems = [];
    let messageState = null;
    let doneSent = false;
    let latestUsage = toOpenAIResponsesUsage();

    const emitEvent = (eventName, payload) => {
      writeSSE(res, {
        event: eventName,
        data: JSON.stringify({
          type: eventName,
          ...payload
        })
      });
    };

    emitEvent("response.created", {
      response: {
        id: responseId,
        object: "response",
        created_at: createdAt,
        model: requestModel,
        status: "in_progress",
        output: [],
        usage: null
      }
    });

    const ensureMessageItem = () => {
      if (messageState) return messageState;
      const item = {
        id: `msg_${randomUUID().replace(/-/g, "")}`,
        type: "message",
        status: "in_progress",
        role: "assistant",
        content: [{ type: "output_text", text: "" }]
      };
      const outputIndex = outputItems.length;
      outputItems.push(item);
      messageState = { item, outputIndex };
      emitEvent("response.output_item.added", {
        output_index: outputIndex,
        item
      });
      return messageState;
    };

    const toolStatesByContentIndex = new Map();
    const ensureToolItem = (payload) => {
      const contentIndex = Number.isInteger(payload?.index) ? payload.index : toolStatesByContentIndex.size;
      let state = toolStatesByContentIndex.get(contentIndex);
      if (state) return state;

      const id = payload?.content_block?.id || `fc_${randomUUID().replace(/-/g, "")}`;
      const item = {
        id,
        type: "function_call",
        status: "in_progress",
        call_id: id,
        name: payload?.content_block?.name || "tool",
        arguments: ""
      };
      const outputIndex = outputItems.length;
      outputItems.push(item);
      state = { item, outputIndex };
      toolStatesByContentIndex.set(contentIndex, state);
      emitEvent("response.output_item.added", {
        output_index: outputIndex,
        item
      });
      return state;
    };

    const emitDone = () => {
      if (doneSent) return;
      doneSent = true;

      for (let outputIndex = 0; outputIndex < outputItems.length; outputIndex += 1) {
        const item = outputItems[outputIndex];
        item.status = "completed";
        emitEvent("response.output_item.done", {
          output_index: outputIndex,
          item
        });
      }

      emitEvent("response.completed", {
        response: {
          id: responseId,
          object: "response",
          created_at: createdAt,
          model: requestModel,
          status: "completed",
          output: outputItems,
          usage: latestUsage
        }
      });

      res.write("data: [DONE]\n\n");
    };

    const parser = createSSEParser(({ event, data }) => {
      if (data === "[DONE]") {
        emitDone();
        return;
      }

      let payload;
      try {
        payload = JSON.parse(data);
      } catch {
        return;
      }

      if (event === "message_delta" && payload.usage) {
        latestUsage = toOpenAIResponsesUsage(payload.usage);
      }

      if (event === "content_block_start" && payload.content_block?.type === "text") {
        ensureMessageItem();
        return;
      }

      if (event === "content_block_delta" && typeof payload.delta?.text === "string" && payload.delta.text.length > 0) {
        const state = ensureMessageItem();
        state.item.content[0].text += payload.delta.text;
        emitEvent("response.output_text.delta", {
          output_index: state.outputIndex,
          item_id: state.item.id,
          content_index: 0,
          delta: payload.delta.text
        });
        return;
      }

      if (event === "content_block_start" && payload.content_block?.type === "tool_use") {
        ensureToolItem(payload);
        return;
      }

      if (
        event === "content_block_delta"
        && payload.delta?.type === "input_json_delta"
        && typeof payload.delta.partial_json === "string"
      ) {
        const state = ensureToolItem(payload);
        state.item.arguments += payload.delta.partial_json;
        emitEvent("response.function_call_arguments.delta", {
          output_index: state.outputIndex,
          item_id: state.item.id,
          delta: payload.delta.partial_json
        });
        return;
      }

      if (event === "message_stop") {
        emitDone();
      }
    });

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.write(decoder.decode(value, { stream: true }));
    }
    parser.end();
    emitDone();
    res.end();
  }

  async bridgeOpenAIToAnthropic(upstreamResponse, res, traceId, requestModel) {
    beginSSE(res, { "x-trace-id": traceId });

    const msgId = `msg_${randomUUID().replace(/-/g, "")}`;

    const reader = upstreamResponse.body.getReader();
    const decoder = new TextDecoder();
    let stopSent = false;
    let started = false;
    let usageSnapshot = null;
    let latestUsage = toAnthropicUsage();
    let finalStopReason = "end_turn";
    let finalDeltaSent = false;
    let nextContentIndex = 0;
    let textBlockIndex = null;
    const toolBlocks = new Map();
    const openedContentBlockIndexes = new Set();

    const emitMessageStart = (usage) => {
      if (started) return;
      writeSSE(res, {
        event: "message_start",
        data: JSON.stringify({
          type: "message_start",
          message: {
            id: msgId,
            type: "message",
            role: "assistant",
            model: requestModel,
            content: [],
            stop_reason: null,
            stop_sequence: null,
            usage: (() => {
              latestUsage = toAnthropicUsage(usage);
              return latestUsage;
            })()
          }
        })
      });
      started = true;
    };

    const emitUsageDelta = (usage) => {
      const mapped = toAnthropicUsage(usage);
      const nextSnapshot = `${mapped.input_tokens}:${mapped.output_tokens}`;
      if (usageSnapshot === nextSnapshot) return;
      usageSnapshot = nextSnapshot;
      latestUsage = mapped;
      writeSSE(res, {
        event: "message_delta",
        data: JSON.stringify({
          type: "message_delta",
          delta: {
            stop_reason: null,
            stop_sequence: null
          },
          usage: mapped
        })
      });
    };

    const emitFinalMessageDelta = () => {
      if (finalDeltaSent) return;
      writeSSE(res, {
        event: "message_delta",
        data: JSON.stringify({
          type: "message_delta",
          delta: {
            stop_reason: finalStopReason,
            stop_sequence: null
          },
          usage: latestUsage
        })
      });
      finalDeltaSent = true;
    };

    const emitContentBlockStart = (index, contentBlock) => {
      if (openedContentBlockIndexes.has(index)) return;
      writeSSE(res, {
        event: "content_block_start",
        data: JSON.stringify({
          type: "content_block_start",
          index,
          content_block: contentBlock
        })
      });
      openedContentBlockIndexes.add(index);
    };

    const emitContentBlockStop = (index) => {
      if (!openedContentBlockIndexes.has(index)) return;
      writeSSE(res, {
        event: "content_block_stop",
        data: JSON.stringify({
          type: "content_block_stop",
          index
        })
      });
      openedContentBlockIndexes.delete(index);
    };

    const ensureTextBlock = () => {
      if (textBlockIndex != null) return textBlockIndex;
      textBlockIndex = nextContentIndex;
      nextContentIndex += 1;
      emitContentBlockStart(textBlockIndex, {
        type: "text",
        text: ""
      });
      return textBlockIndex;
    };

    const ensureToolBlock = (toolCall, position) => {
      const key = Number.isInteger(toolCall?.index) ? `index:${toolCall.index}` : `position:${position}`;
      let state = toolBlocks.get(key);
      if (!state) {
        state = {
          index: nextContentIndex,
          id: toolCall?.id || `toolu_${randomUUID().replace(/-/g, "")}`,
          name: toolCall?.function?.name || "tool"
        };
        nextContentIndex += 1;
        toolBlocks.set(key, state);
        emitContentBlockStart(state.index, {
          type: "tool_use",
          id: state.id,
          name: state.name,
          input: {}
        });
      }
      return state;
    };

    const emitMessageStop = () => {
      if (stopSent) return;
      const indexes = Array.from(openedContentBlockIndexes.keys()).sort((a, b) => a - b);
      for (const index of indexes) {
        emitContentBlockStop(index);
      }
      writeSSE(res, { event: "message_stop", data: JSON.stringify({ type: "message_stop" }) });
      stopSent = true;
    };

    const parser = createSSEParser(({ data }) => {
      if (data === "[DONE]") {
        emitMessageStart();
        emitFinalMessageDelta();
        emitMessageStop();
        return;
      }

      let payload;
      try {
        payload = JSON.parse(data);
      } catch {
        return;
      }

      emitMessageStart(payload.usage);
      if (payload.usage) {
        emitUsageDelta(payload.usage);
      }
      const choice = payload.choices?.[0];
      const delta = choice?.delta || {};

      if (typeof delta.content === "string" && delta.content.length > 0) {
        const index = ensureTextBlock();
        writeSSE(res, {
          event: "content_block_delta",
          data: JSON.stringify({
            type: "content_block_delta",
            index,
            delta: {
              type: "text_delta",
              text: delta.content
            }
          })
        });
      }

      if (Array.isArray(delta.tool_calls)) {
        for (const [position, toolCall] of delta.tool_calls.entries()) {
          const state = ensureToolBlock(toolCall, position);

          const argumentsChunk = toolCall?.function?.arguments;
          if (typeof argumentsChunk === "string" && argumentsChunk.length > 0) {
            writeSSE(res, {
              event: "content_block_delta",
              data: JSON.stringify({
                type: "content_block_delta",
                index: state.index,
                delta: {
                  type: "input_json_delta",
                  partial_json: argumentsChunk
                }
              })
            });
          }
        }
      }

      if (choice?.finish_reason) {
        finalStopReason = toAnthropicStopReason(choice.finish_reason);
      }
    });

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      parser.write(decoder.decode(value, { stream: true }));
    }
    parser.end();
    emitMessageStart();
    emitFinalMessageDelta();
    emitMessageStop();
    res.end();
  }
}

module.exports = {
  ProxyServer,
  __test__: {
    readRequestBody,
    buildUpstreamError,
    mapAnthropicStopReason,
    toStatusCode,
    toAnthropicUsage,
    toAnthropicStopReason
  }
};

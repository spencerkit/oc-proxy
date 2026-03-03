#!/usr/bin/env node

const http = require("node:http")
const { randomUUID } = require("node:crypto")

function parseArgs(argv) {
  const out = {
    host: "127.0.0.1",
    port: 19001,
    chunkDelayMs: 30,
    streamChunks: 24,
  }

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if ((arg === "--host" || arg === "-h") && argv[i + 1]) {
      out.host = String(argv[i + 1])
      i += 1
      continue
    }
    if ((arg === "--port" || arg === "-p") && argv[i + 1]) {
      out.port = Number(argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--chunk-delay-ms" && argv[i + 1]) {
      out.chunkDelayMs = Number(argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--stream-chunks" && argv[i + 1]) {
      out.streamChunks = Number(argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--help") {
      printHelp()
      process.exit(0)
    }
  }

  if (!Number.isFinite(out.port) || out.port <= 0) {
    throw new Error("invalid --port")
  }
  if (!Number.isFinite(out.chunkDelayMs) || out.chunkDelayMs < 0) {
    throw new Error("invalid --chunk-delay-ms")
  }
  if (!Number.isFinite(out.streamChunks) || out.streamChunks < 1) {
    throw new Error("invalid --stream-chunks")
  }

  return out
}

function printHelp() {
  console.log(`Usage:
  node scripts/mock-upstream.js [--host 127.0.0.1] [--port 19001] [--chunk-delay-ms 30] [--stream-chunks 24]
`)
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = []
    req.on("data", chunk => {
      chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
    })
    req.on("end", () => {
      const raw = Buffer.concat(chunks).toString("utf-8")
      if (!raw.trim()) {
        resolve({})
        return
      }
      try {
        resolve(JSON.parse(raw))
      } catch {
        reject(new Error("request body must be valid JSON"))
      }
    })
    req.on("error", reject)
  })
}

function sendJson(res, statusCode, payload, headers = {}) {
  res.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
    ...headers,
  })
  res.end(JSON.stringify(payload))
}

function beginSSE(res) {
  res.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache, no-transform",
    connection: "keep-alive",
    "x-accel-buffering": "no",
  })
}

function writeSSE(res, { event, data }) {
  if (event) {
    res.write(`event: ${event}\n`)
  }
  res.write(`data: ${typeof data === "string" ? data : JSON.stringify(data)}\n\n`)
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms))
}

function makeUsage(index) {
  return {
    prompt_tokens: 12,
    completion_tokens: index,
    total_tokens: 12 + index,
  }
}

async function streamOpenAIResponses(res, body, opts) {
  beginSSE(res)
  const model = body.model || "mock-model"
  const responseId = `resp_${randomUUID().replace(/-/g, "")}`

  writeSSE(res, {
    event: "response.created",
    data: {
      type: "response.created",
      response: {
        id: responseId,
        object: "response",
        model,
        status: "in_progress",
        output: [],
        usage: null,
      },
    },
  })

  for (let i = 1; i <= opts.streamChunks; i += 1) {
    writeSSE(res, {
      event: "response.output_text.delta",
      data: {
        type: "response.output_text.delta",
        delta: `chunk-${i} `,
      },
    })
    await sleep(opts.chunkDelayMs)
  }

  writeSSE(res, {
    event: "response.completed",
    data: {
      type: "response.completed",
      response: {
        id: responseId,
        object: "response",
        status: "completed",
        model,
        output: [
          {
            type: "message",
            role: "assistant",
            content: [{ type: "output_text", text: "mock stream done" }],
          },
        ],
        usage: {
          input_tokens: 12,
          output_tokens: opts.streamChunks,
          total_tokens: 12 + opts.streamChunks,
        },
      },
      usage: makeUsage(opts.streamChunks),
    },
  })
  writeSSE(res, { data: "[DONE]" })
  res.end()
}

async function streamOpenAIChat(res, body, opts) {
  beginSSE(res)
  const id = `chatcmpl_${randomUUID().replace(/-/g, "")}`
  const model = body.model || "mock-model"
  const created = Math.floor(Date.now() / 1000)

  for (let i = 1; i <= opts.streamChunks; i += 1) {
    writeSSE(res, {
      data: {
        id,
        object: "chat.completion.chunk",
        created,
        model,
        choices: [
          {
            index: 0,
            delta:
              i === 1 ? { role: "assistant", content: `chunk-${i} ` } : { content: `chunk-${i} ` },
            finish_reason: null,
          },
        ],
      },
    })
    await sleep(opts.chunkDelayMs)
  }

  writeSSE(res, {
    data: {
      id,
      object: "chat.completion.chunk",
      created,
      model,
      choices: [{ index: 0, delta: {}, finish_reason: "stop" }],
      usage: makeUsage(opts.streamChunks),
    },
  })
  writeSSE(res, { data: "[DONE]" })
  res.end()
}

async function streamAnthropicMessages(res, body, opts) {
  beginSSE(res)
  const msgId = `msg_${randomUUID().replace(/-/g, "")}`
  const model = body.model || "mock-model"

  writeSSE(res, {
    event: "message_start",
    data: {
      type: "message_start",
      message: {
        id: msgId,
        type: "message",
        role: "assistant",
        model,
        content: [],
        stop_reason: null,
        stop_sequence: null,
        usage: {
          input_tokens: 12,
          output_tokens: 0,
        },
      },
    },
  })

  writeSSE(res, {
    event: "content_block_start",
    data: {
      type: "content_block_start",
      index: 0,
      content_block: { type: "text", text: "" },
    },
  })

  for (let i = 1; i <= opts.streamChunks; i += 1) {
    writeSSE(res, {
      event: "content_block_delta",
      data: {
        type: "content_block_delta",
        index: 0,
        delta: {
          type: "text_delta",
          text: `chunk-${i} `,
        },
      },
    })
    await sleep(opts.chunkDelayMs)
  }

  writeSSE(res, {
    event: "content_block_stop",
    data: { type: "content_block_stop", index: 0 },
  })

  writeSSE(res, {
    event: "message_delta",
    data: {
      type: "message_delta",
      delta: {
        stop_reason: "end_turn",
        stop_sequence: null,
      },
      usage: {
        input_tokens: 12,
        output_tokens: opts.streamChunks,
      },
    },
  })
  writeSSE(res, { event: "message_stop", data: { type: "message_stop" } })
  writeSSE(res, { data: "[DONE]" })
  res.end()
}

function nonStreamResponses(body) {
  const model = body.model || "mock-model"
  return {
    id: `resp_${randomUUID().replace(/-/g, "")}`,
    object: "response",
    model,
    status: "completed",
    output: [
      {
        type: "message",
        role: "assistant",
        content: [{ type: "output_text", text: "mock response" }],
      },
    ],
    usage: {
      input_tokens: 12,
      output_tokens: 24,
      total_tokens: 36,
    },
  }
}

function nonStreamChat(body) {
  const model = body.model || "mock-model"
  return {
    id: `chatcmpl_${randomUUID().replace(/-/g, "")}`,
    object: "chat.completion",
    created: Math.floor(Date.now() / 1000),
    model,
    choices: [
      {
        index: 0,
        message: { role: "assistant", content: "mock response" },
        finish_reason: "stop",
      },
    ],
    usage: makeUsage(24),
  }
}

function nonStreamMessages(body) {
  const model = body.model || "mock-model"
  return {
    id: `msg_${randomUUID().replace(/-/g, "")}`,
    type: "message",
    role: "assistant",
    model,
    content: [{ type: "text", text: "mock response" }],
    stop_reason: "end_turn",
    usage: {
      input_tokens: 12,
      output_tokens: 24,
    },
  }
}

async function handleProxyLikeRequest(req, res, body, opts) {
  const stream = !!body.stream
  const route = req.url.split("?")[0]

  if (route === "/v1/responses") {
    if (stream) {
      await streamOpenAIResponses(res, body, opts)
      return
    }
    sendJson(res, 200, nonStreamResponses(body))
    return
  }

  if (route === "/v1/chat/completions") {
    if (stream) {
      await streamOpenAIChat(res, body, opts)
      return
    }
    sendJson(res, 200, nonStreamChat(body))
    return
  }

  if (route === "/v1/messages") {
    if (stream) {
      await streamAnthropicMessages(res, body, opts)
      return
    }
    sendJson(res, 200, nonStreamMessages(body))
    return
  }

  sendJson(res, 404, {
    error: {
      code: "not_found",
      message: `unknown mock route: ${route}`,
    },
  })
}

async function main() {
  const opts = parseArgs(process.argv.slice(2))
  const server = http.createServer(async (req, res) => {
    try {
      const route = req.url.split("?")[0]
      if (req.method === "GET" && route === "/healthz") {
        sendJson(res, 200, { ok: true, service: "mock-upstream" })
        return
      }

      if (req.method !== "POST") {
        sendJson(res, 405, {
          error: {
            code: "method_not_allowed",
            message: "only POST is allowed for mock upstream model endpoints",
          },
        })
        return
      }

      const body = await readBody(req)
      await handleProxyLikeRequest(req, res, body, opts)
    } catch (error) {
      sendJson(res, 400, {
        error: {
          code: "mock_bad_request",
          message: String(error?.message ? error.message : error),
        },
      })
    }
  })

  server.listen(opts.port, opts.host, () => {
    console.log(`[mock-upstream] listening on http://${opts.host}:${opts.port}`)
    console.log(`[mock-upstream] endpoints: /v1/responses /v1/chat/completions /v1/messages`)
  })

  const graceful = () => {
    server.close(() => process.exit(0))
  }
  process.on("SIGINT", graceful)
  process.on("SIGTERM", graceful)
}

main().catch(error => {
  console.error("[mock-upstream] failed:", error)
  process.exit(1)
})

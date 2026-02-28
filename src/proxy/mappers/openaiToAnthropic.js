const SUPPORTED_OPENAI_KEYS = new Set([
  "model",
  "messages",
  "stream",
  "max_tokens",
  "temperature",
  "top_p",
  "tools",
  "tool_choice",
  "metadata",
  "stop",
  "input",
  "system",
  "thinking",
  "context_management"
]);

function assertOpenAICompatibility(body, strictMode) {
  if (!strictMode) return;
  const unknown = Object.keys(body || {}).filter((key) => !SUPPORTED_OPENAI_KEYS.has(key));
  if (unknown.length > 0) {
    const err = new Error(`Unsupported OpenAI fields in strict mode: ${unknown.join(", ")}`);
    err.statusCode = 422;
    throw err;
  }
}

function normalizeOpenAIRequest(path, body) {
  if (path !== "/v1/responses") {
    return body;
  }

  const messages = [];
  if (typeof body.input === "string") {
    messages.push({ role: "user", content: body.input });
  } else if (Array.isArray(body.input)) {
    for (const item of body.input) {
      if (!item) continue;
      if (item.role && item.content != null) {
        messages.push({ role: item.role, content: item.content });
      }
    }
  }

  return {
    model: body.model,
    messages,
    stream: body.stream,
    max_tokens: body.max_tokens,
    temperature: body.temperature,
    top_p: body.top_p,
    tools: body.tools,
    tool_choice: body.tool_choice,
    metadata: body.metadata,
    stop: body.stop,
    system: body.system,
    thinking: body.thinking,
    context_management: body.context_management
  };
}

function toAnthropicContent(content) {
  if (Array.isArray(content)) {
    return content;
  }
  if (typeof content === "string") {
    return [{ type: "text", text: content }];
  }
  if (content == null) {
    return [];
  }
  return [{ type: "text", text: JSON.stringify(content) }];
}

function mapOpenAIToAnthropicRequest(body, { strictMode, targetModel }) {
  assertOpenAICompatibility(body, strictMode);
  const messages = Array.isArray(body.messages) ? body.messages : [];

  const systemChunks = [];
  const anthropicMessages = [];

  for (const msg of messages) {
    if (!msg) continue;
    if (msg.role === "system") {
      if (typeof msg.content === "string") {
        systemChunks.push(msg.content);
      }
      continue;
    }

    if (msg.role === "assistant" && Array.isArray(msg.tool_calls) && msg.tool_calls.length > 0) {
      const content = [];
      if (msg.content) {
        content.push(...toAnthropicContent(msg.content));
      }
      for (const call of msg.tool_calls) {
        content.push({
          type: "tool_use",
          id: call.id,
          name: call.function?.name,
          input: (() => {
            try {
              return JSON.parse(call.function?.arguments || "{}");
            } catch {
              return { raw: call.function?.arguments || "" };
            }
          })()
        });
      }
      anthropicMessages.push({ role: "assistant", content });
      continue;
    }

    anthropicMessages.push({
      role: msg.role,
      content: toAnthropicContent(msg.content)
    });
  }

  const request = {
    model: targetModel || body.model,
    max_tokens: body.max_tokens || 1024,
    temperature: body.temperature,
    top_p: body.top_p,
    stop_sequences: body.stop,
    stream: !!body.stream,
    messages: anthropicMessages
  };

  if (body.system != null) {
    request.system = body.system;
  } else if (systemChunks.length > 0) {
    request.system = systemChunks.join("\n\n");
  }

  if (body.thinking != null) {
    request.thinking = body.thinking;
  }

  if (body.context_management != null) {
    request.context_management = body.context_management;
  }

  if (Array.isArray(body.tools)) {
    request.tools = body.tools.map((tool) => ({
      name: tool.function?.name,
      description: tool.function?.description,
      input_schema: tool.function?.parameters || { type: "object", properties: {} }
    }));
  }

  if (body.tool_choice && typeof body.tool_choice === "object") {
    request.tool_choice = {
      type: body.tool_choice.type || "auto",
      name: body.tool_choice.function?.name
    };
  }

  return request;
}

function mapAnthropicToOpenAIResponse(anthropicResponse, { requestModel }) {
  const content = [];
  const toolCalls = [];

  for (const block of anthropicResponse.content || []) {
    if (!block) continue;
    if (block.type === "text") {
      content.push(block.text || "");
      continue;
    }
    if (block.type === "tool_use") {
      toolCalls.push({
        id: block.id || `tool_${Math.random().toString(36).slice(2)}`,
        type: "function",
        function: {
          name: block.name,
          arguments: JSON.stringify(block.input || {})
        }
      });
    }
  }

  const message = {
    role: "assistant",
    content: content.join("")
  };

  if (toolCalls.length > 0) {
    message.tool_calls = toolCalls;
  }

  return {
    id: anthropicResponse.id || `chatcmpl_${Math.random().toString(36).slice(2)}`,
    object: "chat.completion",
    created: Math.floor(Date.now() / 1000),
    model: requestModel || anthropicResponse.model,
    choices: [
      {
        index: 0,
        message,
        finish_reason: toolCalls.length > 0 ? "tool_calls" : "stop"
      }
    ],
    usage: {
      prompt_tokens: anthropicResponse.usage?.input_tokens || 0,
      completion_tokens: anthropicResponse.usage?.output_tokens || 0,
      total_tokens: (anthropicResponse.usage?.input_tokens || 0) + (anthropicResponse.usage?.output_tokens || 0)
    }
  };
}

function mapOpenAIChatToResponses(chatResponse) {
  const text = chatResponse.choices?.[0]?.message?.content || "";
  return {
    id: chatResponse.id,
    object: "response",
    created_at: chatResponse.created,
    model: chatResponse.model,
    output: [
      {
        type: "message",
        role: "assistant",
        content: [
          {
            type: "output_text",
            text
          }
        ]
      }
    ],
    usage: chatResponse.usage || {}
  };
}

module.exports = {
  normalizeOpenAIRequest,
  mapOpenAIToAnthropicRequest,
  mapAnthropicToOpenAIResponse,
  mapOpenAIChatToResponses
};

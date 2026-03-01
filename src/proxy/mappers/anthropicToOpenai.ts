// @ts-nocheck
const SUPPORTED_ANTHROPIC_KEYS = new Set([
  "model",
  "messages",
  "max_tokens",
  "system",
  "temperature",
  "top_p",
  "stream",
  "tools",
  "tool_choice",
  "stop_sequences",
  "metadata",
  "thinking",
  "context_management"
]);

function assertAnthropicCompatibility(body, strictMode) {
  if (!strictMode) return;
  const unknown = Object.keys(body || {}).filter((key) => !SUPPORTED_ANTHROPIC_KEYS.has(key));
  if (unknown.length > 0) {
    const err = new Error(`Unsupported Claude fields in strict mode: ${unknown.join(", ")}`);
    err.statusCode = 422;
    throw err;
  }
}

function flattenAnthropicContent(content) {
  if (!Array.isArray(content)) {
    return typeof content === "string" ? content : "";
  }

  const texts = [];
  for (const block of content) {
    if (block?.type === "text" && block.text) {
      texts.push(block.text);
    }
  }
  return texts.join("");
}

function toOpenAIToolResultContent(content) {
  if (content == null) {
    return "";
  }
  if (typeof content === "string") {
    return content;
  }
  if (Array.isArray(content)) {
    const texts = [];
    for (const block of content) {
      if (block?.type === "text" && typeof block.text === "string") {
        texts.push(block.text);
      }
    }
    if (texts.length > 0) {
      return texts.join("");
    }
  }
  return JSON.stringify(content);
}

function toAnthropicStopReason(finishReason) {
  if (finishReason === "tool_calls") return "tool_use";
  if (finishReason === "length") return "max_tokens";
  return "end_turn";
}

function mapAnthropicToOpenAIRequest(body, { strictMode, targetModel }) {
  assertAnthropicCompatibility(body, strictMode);

  const messages = [];
  if (body.system) {
    messages.push({ role: "system", content: body.system });
  }

  for (const msg of body.messages || []) {
    if (!msg) continue;
    const content = Array.isArray(msg.content)
      ? msg.content
      : (msg.content == null
        ? []
        : [{ type: "text", text: typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content) }]);
    const text = flattenAnthropicContent(content);

    if (msg.role === "assistant") {
      const toolCalls = [];
      for (const block of content) {
        if (block?.type === "tool_use") {
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
      const assistantMsg = { role: "assistant", content: text };
      if (toolCalls.length > 0) {
        assistantMsg.tool_calls = toolCalls;
      }
      messages.push(assistantMsg);
      continue;
    }

    if (msg.role === "user") {
      const textParts = [];
      const flushTextParts = () => {
        if (textParts.length === 0) return;
        messages.push({ role: "user", content: textParts.join("") });
        textParts.length = 0;
      };

      for (const block of content) {
        if (!block) continue;
        if (block.type === "tool_result") {
          flushTextParts();
          messages.push({
            role: "tool",
            tool_call_id: block.tool_use_id || `tool_${Math.random().toString(36).slice(2)}`,
            content: toOpenAIToolResultContent(block.content)
          });
          continue;
        }
        if (block.type === "text") {
          textParts.push(block.text || "");
        }
      }

      flushTextParts();
      continue;
    }

    messages.push({ role: msg.role, content: text });
  }

  const request = {
    model: targetModel || body.model,
    messages,
    max_tokens: body.max_tokens,
    temperature: body.temperature,
    top_p: body.top_p,
    stream: !!body.stream
  };

  if (Array.isArray(body.tools)) {
    request.tools = body.tools.map((tool) => ({
      type: "function",
      function: {
        name: tool.name,
        description: tool.description,
        parameters: tool.input_schema || { type: "object", properties: {} }
      }
    }));
  }

  if (body.tool_choice && body.tool_choice.name) {
    request.tool_choice = {
      type: "function",
      function: {
        name: body.tool_choice.name
      }
    };
  }

  if (Array.isArray(body.stop_sequences)) {
    request.stop = body.stop_sequences;
  }

  return request;
}

function mapOpenAIToAnthropicResponse(openaiResponse, { requestModel }) {
  const choice = openaiResponse.choices?.[0] || {};
  const message = choice.message || {};
  const content = [];

  if (message.content) {
    content.push({ type: "text", text: message.content });
  }

  for (const call of message.tool_calls || []) {
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

  return {
    id: openaiResponse.id || `msg_${Math.random().toString(36).slice(2)}`,
    type: "message",
    role: "assistant",
    model: requestModel || openaiResponse.model,
    content,
    stop_reason: toAnthropicStopReason(choice.finish_reason),
    usage: {
      input_tokens: openaiResponse.usage?.prompt_tokens || 0,
      output_tokens: openaiResponse.usage?.completion_tokens || 0
    }
  };
}

module.exports = {
  mapAnthropicToOpenAIRequest,
  mapOpenAIToAnthropicResponse
};

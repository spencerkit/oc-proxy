const SUPPORTED_OPENAI_KEYS = new Set([
  "model",
  "messages",
  "stream",
  "max_tokens",
  "max_output_tokens",
  "temperature",
  "top_p",
  "tools",
  "tool_choice",
  "parallel_tool_calls",
  "metadata",
  "stop",
  "input",
  "instructions",
  "reasoning",
  "truncation",
  "previous_response_id",
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
  const input = body.input;

  const toText = (value) => {
    if (value == null) return "";
    if (typeof value === "string") return value;
    if (Array.isArray(value)) {
      const chunks = [];
      for (const part of value) {
        if (part == null) continue;
        if (typeof part === "string") {
          chunks.push(part);
          continue;
        }
        if (typeof part !== "object") {
          chunks.push(String(part));
          continue;
        }
        if (typeof part.text === "string") {
          chunks.push(part.text);
          continue;
        }
        if (typeof part.output_text === "string") {
          chunks.push(part.output_text);
          continue;
        }
        if (typeof part.input_text === "string") {
          chunks.push(part.input_text);
          continue;
        }
      }
      if (chunks.length > 0) {
        return chunks.join("");
      }
    }
    return JSON.stringify(value);
  };

  const toFunctionArguments = (value) => {
    if (typeof value === "string") {
      return value;
    }
    return JSON.stringify(value || {});
  };

  const pushInputItemAsMessage = (item) => {
    if (!item) return;

    if (item.type === "function_call") {
      messages.push({
        role: "assistant",
        content: "",
        tool_calls: [{
          id: item.call_id || item.id || `call_${Math.random().toString(36).slice(2)}`,
          type: "function",
          function: {
            name: item.name || item.function?.name || "tool",
            arguments: toFunctionArguments(item.arguments ?? item.function?.arguments)
          }
        }]
      });
      return;
    }

    if (item.type === "function_call_output") {
      messages.push({
        role: "tool",
        tool_call_id: item.call_id || item.id || `call_${Math.random().toString(36).slice(2)}`,
        content: toText(item.output ?? item.content)
      });
      return;
    }

    const role = item.role || (item.type === "message" ? "user" : null);
    if (role) {
      messages.push({
        role,
        content: item.content == null ? "" : item.content
      });
      return;
    }

    if (item.type === "input_text" && typeof item.text === "string") {
      messages.push({ role: "user", content: item.text });
    }
  };

  if (typeof input === "string") {
    messages.push({ role: "user", content: input });
  } else if (Array.isArray(input)) {
    for (const item of input) {
      pushInputItemAsMessage(item);
    }
  } else if (input && typeof input === "object") {
    pushInputItemAsMessage(input);
  }

  return {
    model: body.model,
    messages,
    stream: body.stream,
    max_tokens: body.max_tokens ?? body.max_output_tokens,
    temperature: body.temperature,
    top_p: body.top_p,
    tools: body.tools,
    tool_choice: body.tool_choice,
    metadata: body.metadata,
    stop: body.stop,
    system: body.system ?? body.instructions,
    thinking: body.thinking,
    context_management: body.context_management
  };
}

function toAnthropicContent(content) {
  if (Array.isArray(content)) {
    const blocks = [];
    for (const item of content) {
      if (item == null) continue;
      if (typeof item === "string") {
        if (item.length > 0) {
          blocks.push({ type: "text", text: item });
        }
        continue;
      }
      if (typeof item !== "object") {
        blocks.push({ type: "text", text: String(item) });
        continue;
      }
      if (item.type === "text") {
        blocks.push(item);
        continue;
      }
      if ((item.type === "input_text" || item.type === "output_text") && typeof item.text === "string") {
        blocks.push({ type: "text", text: item.text });
        continue;
      }
      if (typeof item.text === "string") {
        blocks.push({ type: "text", text: item.text });
        continue;
      }
      blocks.push({ type: "text", text: JSON.stringify(item) });
    }
    return blocks;
  }
  if (typeof content === "string") {
    return [{ type: "text", text: content }];
  }
  if (content == null) {
    return [];
  }
  return [{ type: "text", text: JSON.stringify(content) }];
}

function toAnthropicToolResultContent(content) {
  if (content == null) {
    return "";
  }
  if (typeof content === "string") {
    return content;
  }
  if (Array.isArray(content)) {
    const textBlocks = content
      .filter((item) => item && typeof item === "object")
      .map((item) => {
        if (item.type === "text") return item.text || "";
        if ((item.type === "input_text" || item.type === "output_text") && typeof item.text === "string") {
          return item.text;
        }
        return "";
      })
      .filter(Boolean);
    if (textBlocks.length > 0) {
      return textBlocks.join("");
    }
  }
  return JSON.stringify(content);
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

    if (msg.role === "tool") {
      const toolUseId = typeof msg.tool_call_id === "string" && msg.tool_call_id.trim()
        ? msg.tool_call_id.trim()
        : `toolu_${Math.random().toString(36).slice(2)}`;
      anthropicMessages.push({
        role: "user",
        content: [{
          type: "tool_result",
          tool_use_id: toolUseId,
          content: toAnthropicToolResultContent(msg.content)
        }]
      });
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
      name: tool.function?.name || tool.name,
      description: tool.function?.description || tool.description,
      input_schema: tool.function?.parameters || tool.parameters || tool.input_schema || { type: "object", properties: {} }
    }));
  }

  if (body.tool_choice != null) {
    if (typeof body.tool_choice === "string") {
      request.tool_choice = {
        type: body.tool_choice
      };
    } else if (typeof body.tool_choice === "object") {
      request.tool_choice = {
        type: body.tool_choice.type || "auto",
        name: body.tool_choice.function?.name || body.tool_choice.name
      };
    }
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
  const toolCalls = Array.isArray(chatResponse.choices?.[0]?.message?.tool_calls)
    ? chatResponse.choices[0].message.tool_calls
    : [];

  const output = [
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
  ];

  for (const toolCall of toolCalls) {
    output.push({
      type: "function_call",
      id: toolCall.id,
      call_id: toolCall.id,
      status: "completed",
      name: toolCall.function?.name,
      arguments: toolCall.function?.arguments || "{}"
    });
  }

  const usage = chatResponse.usage || {};
  const inputTokens = usage.input_tokens ?? usage.prompt_tokens ?? 0;
  const outputTokens = usage.output_tokens ?? usage.completion_tokens ?? 0;

  return {
    id: chatResponse.id,
    object: "response",
    created_at: chatResponse.created,
    model: chatResponse.model,
    status: "completed",
    output,
    usage: {
      input_tokens: inputTokens,
      output_tokens: outputTokens,
      total_tokens: usage.total_tokens ?? (inputTokens + outputTokens)
    }
  };
}

module.exports = {
  normalizeOpenAIRequest,
  mapOpenAIToAnthropicRequest,
  mapAnthropicToOpenAIResponse,
  mapOpenAIChatToResponses
};

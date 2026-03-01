// @ts-nocheck
function createSSEParser(onEvent) {
  let buffer = "";
  let currentEvent = "message";
  let dataLines = [];

  function flushEvent() {
    if (dataLines.length === 0) {
      currentEvent = "message";
      return;
    }
    const payload = dataLines.join("\n");
    onEvent({ event: currentEvent, data: payload });
    currentEvent = "message";
    dataLines = [];
  }

  function write(chunk) {
    buffer += chunk;
    let idx;
    while ((idx = buffer.indexOf("\n")) !== -1) {
      const raw = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 1);
      const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;

      if (line === "") {
        flushEvent();
        continue;
      }

      if (line.startsWith(":")) {
        continue;
      }

      if (line.startsWith("event:")) {
        currentEvent = line.slice(6).trim();
        continue;
      }

      if (line.startsWith("data:")) {
        dataLines.push(line.slice(5).trimStart());
      }
    }
  }

  function end() {
    flushEvent();
  }

  return {
    write,
    end
  };
}

function writeSSE(res, { event, data }) {
  if (event) {
    res.write(`event: ${event}\n`);
  }
  const lines = String(data).split("\n");
  for (const line of lines) {
    res.write(`data: ${line}\n`);
  }
  res.write("\n");
}

function beginSSE(res, extraHeaders = {}) {
  res.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache, no-transform",
    connection: "keep-alive",
    ...extraHeaders
  });
}

module.exports = {
  createSSEParser,
  writeSSE,
  beginSSE
};

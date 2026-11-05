import { createServer } from "node:http";

const requestHeaders = (message) => {
  const headers = [];
  for (let index = 0; index < message.rawHeaders.length; index += 2) {
    headers.push({ name: message.rawHeaders[index].toLowerCase(), value: message.rawHeaders[index + 1] });
  }
  return headers;
};

const requestTarget = (request) => {
  try {
    return new URL(request.url, "http://localhost");
  } catch {
    return null;
  }
};

const rejectMalformedTarget = (request, response) => {
  request.resume();
  response.writeHead(400, { "content-type": "text/plain", connection: "close" });
  response.end("Bad Request");
};

const write = (response, serverResponse) => {
  const headers = {};
  for (const header of serverResponse.headers) {
    headers[header.name] = header.value;
  }
  try {
    response.writeHead(serverResponse.status, headers);
  } catch {
    response.writeHead(500, { "content-type": "text/plain" });
    response.end("Internal Server Error");
    return;
  }
  response.end(serverResponse.body);
};

const incoming = (request, response, url, body) => {
  let disconnected = false;
  let onDisconnect = null;
  response.on("close", () => {
    if (!response.writableFinished) {
      disconnected = true;
      if (onDisconnect !== null) {
        onDisconnect();
      }
    }
  });
  return {
    request: {
      method: request.method,
      path: url.pathname,
      query: url.search.slice(1),
      headers: requestHeaders(request),
      body,
    },
    respond: (serverResponse) => () => {
      if (!disconnected) {
        write(response, serverResponse);
      }
    },
    onDisconnect: (effect) => () => {
      if (disconnected) {
        effect();
      } else {
        onDisconnect = effect;
      }
    },
  };
};

export const listenNode = (address) => (onIncoming) => (onError) => (onListening) => () => {
  const server = createServer((request, response) => {
    const url = requestTarget(request);
    if (url === null) {
      rejectMalformedTarget(request, response);
      return;
    }
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      onIncoming(incoming(request, response, url, Buffer.concat(chunks).toString("utf8")))();
    });
  });
  server.once("error", (error) => onError(String(error))());
  try {
    server.listen(address.port, address.host, () => {
      const actual = server.address();
      onListening({
        address: { host: actual.address, port: actual.port },
        close: (callback) => () => {
          server.close(() => callback());
          server.closeIdleConnections();
        },
      })();
    });
  } catch (error) {
    onError(String(error))();
  }
  return () => {
    server.close();
  };
};

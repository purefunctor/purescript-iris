import { Agent, request as httpRequest } from "node:http";
import { Agent as SecureAgent, request as httpsRequest } from "node:https";

const agent = new Agent({ keepAlive: true });
const secureAgent = new SecureAgent({ keepAlive: true });

const headerObject = (headers) => {
  const object = {};
  for (const header of headers) {
    object[header.name] = header.value;
  }
  return object;
};

const responseHeaders = (message) => {
  const headers = [];
  for (let index = 0; index < message.rawHeaders.length; index += 2) {
    headers.push({ name: message.rawHeaders[index].toLowerCase(), value: message.rawHeaders[index + 1] });
  }
  return headers;
};

const send = (request, onResponse, onError) => {
  const url = new URL(request.url);
  const secure = url.protocol === "https:";
  const options = {
    method: request.method,
    headers: headerObject(request.headers),
    agent: secure ? secureAgent : agent,
  };
  const outgoing = (secure ? httpsRequest : httpRequest)(url, options, onResponse);
  outgoing.on("error", onError);
  outgoing.end(request.hasBody ? request.body : undefined);
  return outgoing;
};

export const keepAliveClient = (request) => (onError) => (onSuccess) => () => {
  let settled = false;
  const fail = (error) => {
    if (!settled) {
      settled = true;
      onError(String(error))();
    }
  };
  const receive = (response) => {
    const chunks = [];
    response.on("data", (chunk) => chunks.push(chunk));
    response.on("error", fail);
    response.on("end", () => {
      if (!settled) {
        settled = true;
        onSuccess({
          status: response.statusCode,
          headers: responseHeaders(response),
          body: Buffer.concat(chunks).toString("utf8"),
        })();
      }
    });
  };
  let outgoing = null;
  try {
    outgoing = send(request, receive, fail);
  } catch (error) {
    fail(error);
  }
  return () => {
    settled = true;
    if (outgoing !== null) {
      outgoing.destroy();
    }
  };
};

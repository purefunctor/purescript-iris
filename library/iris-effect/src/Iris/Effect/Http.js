const headerObject = (headers) => {
  const object = {};
  for (const header of headers) {
    object[header.name] = header.value;
  }
  return object;
};

const readResponse = async (response) => {
  const body = await response.text();
  const headers = [];
  response.headers.forEach((value, name) => headers.push({ name, value }));
  return { status: response.status, headers, body };
};

export const fetchClient = (request) => (onError) => (onSuccess) => () => {
  const controller = new AbortController();
  fetch(request.url, {
    method: request.method,
    headers: headerObject(request.headers),
    body: request.hasBody ? request.body : undefined,
    signal: controller.signal,
  })
    .then(readResponse)
    .then(
      (response) => {
        if (!controller.signal.aborted) {
          onSuccess(response)();
        }
      },
      (error) => {
        if (!controller.signal.aborted) {
          onError(String(error))();
        }
      },
    );
  return () => {
    controller.abort();
  };
};

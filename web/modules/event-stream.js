export function connectEventStream(options) {
  let socket = null;
  let reconnectTimer = null;
  let stopped = false;
  let lastSeq = Number(options.lastSeq || 0);

  const connect = () => {
    if (stopped) {
      return;
    }
    socket = new WebSocket(options.url);
    socket.addEventListener("open", () => options.onOpen?.());
    socket.addEventListener("message", (event) => {
      let payload;
      try {
        payload = JSON.parse(event.data);
      } catch (_error) {
        options.onInvalid?.(event.data);
        return;
      }
      const seq = Number(payload.seq || 0);
      if (seq && seq <= lastSeq) {
        return;
      }
      lastSeq = Math.max(lastSeq, seq);
      options.onEvent?.(payload);
    });
    socket.addEventListener("close", () => {
      options.onClose?.();
      if (!stopped) {
        reconnectTimer = window.setTimeout(connect, options.reconnectDelayMs || 1500);
      }
    });
    socket.addEventListener("error", () => options.onError?.());
  };

  connect();
  return {
    stop() {
      stopped = true;
      if (reconnectTimer) {
        window.clearTimeout(reconnectTimer);
      }
      socket?.close();
    },
    lastSequence() {
      return lastSeq;
    },
  };
}

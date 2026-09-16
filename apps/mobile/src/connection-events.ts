/** Keep live events connected after Wi-Fi changes or a desktop restart. */
export function subscribeToDesktop(
  url: string,
  callbacks: {
    onConnected: () => void;
    onInterrupted: () => void;
    onMessage: (data: string) => void;
  },
) {
  let stopped = false;
  let attempt = 0;
  let socket: WebSocket | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;
  const schedule = () => {
    if (stopped || retry) return;
    callbacks.onInterrupted();
    retry = setTimeout(
      () => {
        retry = undefined;
        connect();
      },
      Math.min(1000 * 2 ** attempt++, 15_000),
    );
  };
  const connect = () => {
    if (stopped) return;
    try {
      const current = new WebSocket(url);
      socket = current;
      current.onopen = () => {
        if (stopped || socket !== current) return;
        attempt = 0;
        callbacks.onConnected();
      };
      current.onmessage = (event) => {
        if (!stopped && socket === current) callbacks.onMessage(String(event.data));
      };
      const failed = () => {
        if (stopped || socket !== current) return;
        socket = undefined;
        current.onclose = null;
        current.onerror = null;
        current.close();
        schedule();
      };
      current.onclose = failed;
      current.onerror = failed;
    } catch {
      schedule();
    }
  };
  connect();
  return () => {
    stopped = true;
    if (retry) clearTimeout(retry);
    if (socket) {
      socket.onclose = null;
      socket.onerror = null;
      socket.close();
    }
  };
}

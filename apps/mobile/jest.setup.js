global.IS_REACT_ACT_ENVIRONMENT = true;

class MockWebSocket {
  static lastUrl = "";
  onmessage = null;
  onopen = null;
  onclose = null;
  constructor(url) {
    MockWebSocket.lastUrl = String(url);
  }
  close() {}
}

global.WebSocket = MockWebSocket;

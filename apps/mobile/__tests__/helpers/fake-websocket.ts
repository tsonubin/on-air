import type { WsEvent } from "@on-air/api-types";
import type { WebSocketLike } from "@on-air/control-client";

/** A scriptable WebSocket: tests open it, push events, and drop it. */
export class FakeWebSocket implements WebSocketLike {
  static instances: FakeWebSocket[] = [];
  static get last(): FakeWebSocket | undefined {
    return FakeWebSocket.instances[FakeWebSocket.instances.length - 1];
  }
  static reset(): void {
    FakeWebSocket.instances = [];
  }

  onopen: WebSocketLike["onopen"] = null;
  onmessage: WebSocketLike["onmessage"] = null;
  onclose: WebSocketLike["onclose"] = null;
  onerror: WebSocketLike["onerror"] = null;
  closed = false;
  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }

  close(): void {
    this.closed = true;
  }

  open(): void {
    this.onopen?.({});
  }

  emit(event: WsEvent): void {
    this.onmessage?.({ data: JSON.stringify(event) });
  }

  dropFromServer(): void {
    this.onclose?.({ code: 1006 });
  }
}

import { useEffect, useRef, useCallback, useState } from "react";

interface WSMessage {
  jsonrpc: string;
  method: string;
  params: { result: unknown; subscription: string };
}

export function useBlockSubscription(onBlock: (block: unknown) => void) {
  const wsRef = useRef<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);

  const connect = useCallback(() => {
    const ws = new WebSocket("ws://178.104.202.101:9944");
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      ws.send(
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "polay_subscribeNewBlocks",
          params: [],
        })
      );
    };

    ws.onmessage = (event) => {
      try {
        const msg: WSMessage = JSON.parse(event.data);
        if (msg.method === "polay_newBlock" && msg.params?.result) {
          onBlock(msg.params.result);
        }
      } catch {
        // ignore parse errors
      }
    };

    ws.onclose = () => {
      setConnected(false);
      setTimeout(connect, 3000);
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [onBlock]);

  useEffect(() => {
    connect();
    return () => {
      wsRef.current?.close();
    };
  }, [connect]);

  return connected;
}

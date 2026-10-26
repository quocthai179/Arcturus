import { useCallback, useEffect, useReducer, useRef } from 'react';
import { connectionInfo } from './hub';
import type { ClientMsg } from './protocol';
import { initialState, reducer } from './store';
import { HubClient } from './ws';

export function useHubConnection() {
  const [state, dispatch] = useReducer(reducer, initialState);
  const client = useRef<HubClient | null>(null);
  useEffect(() => {
    const c = new HubClient({
      connection: connectionInfo,
      onMessage: (msg) => dispatch({ type: 'server', msg }),
      onStatus: (status, error) => dispatch({ type: 'connection', status, error }),
    });
    client.current = c;
    c.start();
    return () => {
      c.stop();
      client.current = null;
    };
  }, []);
  const send = useCallback((msg: ClientMsg) => client.current?.send(msg) ?? false, []);
  return { state, send };
}

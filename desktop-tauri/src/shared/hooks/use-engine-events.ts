import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';

export type EngineEventType = 'instance_changed' | 'task_changed' | 'definition_changed';

interface EngineEventPayload {
  type: EngineEventType;
}

/**
 * Abonniert Tauri-Engine-Events und ruft bei jedem passenden Event `callback` auf.
 *
 * @param callback Funktion, die bei einem Engine-Event aufgerufen wird.
 * @param filter   Optionale Liste von Event-Typen. Wenn leer/nicht angegeben, werden alle Events verarbeitet.
 * @param enabled  Ob das Abonnement aktiv ist (Standard: true).
 */
export function useEngineEvents(
  callback: () => void,
  filter?: EngineEventType[],
  enabled: boolean = true,
) {
  const callbackRef = useRef(callback);
  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  useEffect(() => {
    if (!enabled) return;

    let unlisten: (() => void) | undefined;

    listen<EngineEventPayload>('engine-event', (event) => {
      const eventType = event.payload?.type;
      if (!filter || filter.length === 0 || (eventType && filter.includes(eventType))) {
        callbackRef.current();
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [enabled, filter?.join(',')]);
}

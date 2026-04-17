import { useState, useEffect } from 'react';
import { ScrollText, Loader2 } from 'lucide-react';
import { getInstanceHistory, type HistoryEntry } from '../../../shared/lib/tauri';

interface LogsTabProps {
  instanceId: string;
  auditLog: string[];
  refreshTrigger?: number;
}

function formatTs(iso: string): { date: string; time: string } {
  const d = new Date(iso);
  return {
    date: d.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' }),
    time: d.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit', second: '2-digit' }),
  };
}

// Emoji-Präfix aus dem audit_log-String extrahieren, falls vorhanden
function extractEmoji(s: string): { prefix: string; body: string } {
  const m = s.match(/^([\p{Emoji_Presentation}\p{Extended_Pictographic}▶⚙✅🔗🎯⏰◆•]+)\s*(.*)/u);
  if (m) return { prefix: m[1], body: m[2] };
  return { prefix: '', body: s };
}

export function LogsTab({ instanceId, auditLog, refreshTrigger = 0 }: LogsTabProps) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getInstanceHistory(instanceId, {})
      .then(data => { if (!cancelled) setEntries(data); })
      .catch(() => { /* Fallback auf audit_log */ })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [instanceId, refreshTrigger]);

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center text-muted-foreground gap-2">
        <Loader2 className="h-4 w-4 animate-spin" />
        <span className="text-sm">Lade Logs…</span>
      </div>
    );
  }

  // History-Einträge mit Timestamps bevorzugen
  if (entries.length > 0) {
    return (
      <div className="p-4 space-y-0.5">
        {[...entries].reverse().map(entry => {
          const { date, time } = formatTs(entry.timestamp);
          return (
            <div
              key={entry.id}
              className="grid grid-cols-[auto_auto_1fr] gap-x-4 items-baseline px-3 py-1.5 rounded-md hover:bg-muted/50 transition-colors font-mono text-xs"
            >
              <span className="text-muted-foreground tabular-nums whitespace-nowrap">{date}</span>
              <span className="text-muted-foreground/80 tabular-nums font-semibold whitespace-nowrap">{time}</span>
              <span className="text-foreground/80 break-words">
                {entry.description}
                {entry.node_id && (
                  <span className="ml-2 text-muted-foreground/60">({entry.node_id})</span>
                )}
              </span>
            </div>
          );
        })}
      </div>
    );
  }

  // Fallback: audit_log-Strings ohne Timestamps
  if (auditLog.length === 0) {
    return (
      <div className="p-6 flex flex-col items-center justify-center text-muted-foreground gap-2 py-16">
        <ScrollText className="h-8 w-8 opacity-30" />
        <span className="text-sm">Kein Log vorhanden</span>
      </div>
    );
  }

  return (
    <div className="p-4 space-y-0.5">
      {[...auditLog].reverse().map((line, i) => {
        const { prefix, body } = extractEmoji(line);
        return (
          <div
            key={i}
            className="flex items-start gap-3 px-3 py-1.5 rounded-md hover:bg-muted/50 transition-colors font-mono text-xs"
          >
            {prefix && <span className="shrink-0">{prefix}</span>}
            <span className="text-foreground/80 break-words">{body}</span>
          </div>
        );
      })}
    </div>
  );
}

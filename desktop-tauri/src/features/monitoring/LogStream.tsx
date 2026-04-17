import { useState, useEffect, useRef, useCallback } from 'react';
import { getLogEntries } from '../../shared/lib/tauri';
import type { LogEntry } from '../../shared/lib/tauri';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Terminal, Pause, Play, Trash2, ArrowDown } from 'lucide-react';
import { cn } from '@/lib/utils';

const LEVEL_STYLES: Record<string, string> = {
  ERROR: 'bg-red-500/15 text-red-600 dark:text-red-400 border-red-500/30',
  WARN:  'bg-yellow-500/15 text-yellow-700 dark:text-yellow-400 border-yellow-500/30',
  INFO:  'bg-blue-500/15 text-blue-700 dark:text-blue-400 border-blue-500/30',
  DEBUG: 'bg-muted text-muted-foreground border-border',
  TRACE: 'bg-muted/50 text-muted-foreground/70 border-border/50',
};

const LEVEL_ROW: Record<string, string> = {
  ERROR: 'border-l-2 border-red-500/50 bg-red-500/5',
  WARN:  'border-l-2 border-yellow-500/50 bg-yellow-500/5',
  INFO:  '',
  DEBUG: 'opacity-75',
  TRACE: 'opacity-50',
};

export function LogStream() {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [levelFilter, setLevelFilter] = useState<string>('info');
  const [search, setSearch] = useState('');
  const [paused, setPaused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showScrollBtn, setShowScrollBtn] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const isAtBottomRef = useRef(true);

  const load = useCallback(async () => {
    if (paused) return;
    try {
      const data = await getLogEntries(levelFilter, search || undefined, 500);
      setEntries(data);
      setError(null);
    } catch (e: any) {
      setError(String(e));
    }
  }, [paused, levelFilter, search]);

  // Auto-scroll: instant (kein Animation-Flicker bei 2s-Polling), nur wenn User bereits am Ende war
  useEffect(() => {
    if (isAtBottomRef.current && !paused && bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: 'instant' as ScrollBehavior });
    }
  }, [entries, paused]);

  // Polling alle 2 Sekunden
  useEffect(() => {
    load();
    const id = setInterval(load, 2000);
    return () => clearInterval(id);
  }, [load]);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
    isAtBottomRef.current = atBottom;
    setShowScrollBtn(!atBottom);
  };

  const scrollToBottom = () => {
    bottomRef.current?.scrollIntoView({ behavior: 'instant' as ScrollBehavior });
    isAtBottomRef.current = true;
    setShowScrollBtn(false);
  };

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center gap-2 p-3 border-b bg-muted/20 flex-wrap">
        <Terminal className="h-4 w-4 text-primary shrink-0" />
        <span className="font-semibold text-sm shrink-0">Log Stream</span>

        <select
          value={levelFilter}
          onChange={e => { setLevelFilter(e.target.value); isAtBottomRef.current = true; }}
          className="h-8 rounded-md border border-input bg-background px-2 text-xs focus:outline-none focus:ring-1 focus:ring-ring"
        >
          <option value="error">ERROR+</option>
          <option value="warn">WARN+</option>
          <option value="info">INFO+</option>
          <option value="debug">DEBUG+</option>
          <option value="trace">TRACE+</option>
        </select>

        <Input
          className="h-8 text-xs w-[200px] font-mono"
          placeholder="Filter…"
          value={search}
          onChange={e => { setSearch(e.target.value); isAtBottomRef.current = true; }}
        />

        <div className="ml-auto flex items-center gap-2">
          <Badge variant="secondary" className="text-xs tabular-nums">
            {entries.length} Einträge
          </Badge>
          <Button
            size="sm"
            variant="outline"
            className="h-8 gap-1.5 text-xs"
            onClick={() => setPaused(p => !p)}
          >
            {paused ? <><Play className="h-3 w-3" /> Fortsetzen</> : <><Pause className="h-3 w-3" /> Pause</>}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-8 text-muted-foreground"
            onClick={() => setEntries([])}
            title="Anzeige leeren"
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        </div>
      </div>

      {error && (
        <div className="text-xs text-destructive px-3 py-1 bg-destructive/10">
          Fehler: {error}
        </div>
      )}

      {/* Log-Liste */}
      <div className="relative flex-1 min-h-0 flex flex-col">
        {showScrollBtn && (
          <button
            onClick={scrollToBottom}
            className="absolute bottom-4 right-4 z-10 flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium shadow-lg bg-primary text-primary-foreground hover:bg-primary/90 transition-all animate-in fade-in slide-in-from-bottom-2"
          >
            <ArrowDown className="h-3 w-3" />
            Neueste Logs
          </button>
        )}
      <div
        ref={scrollAreaRef}
        className="flex-1 overflow-y-auto font-mono text-xs"
        onScroll={handleScroll}
      >
        {entries.length === 0 ? (
          <div className="text-center text-muted-foreground py-12 italic">
            Keine Log-Einträge vorhanden.
          </div>
        ) : (
          <table className="w-full border-collapse">
            <tbody>
              {[...entries].reverse().map((e, i) => (
                <tr
                  key={i}
                  className={cn(
                    'hover:bg-accent/30 transition-colors',
                    LEVEL_ROW[e.level] ?? ''
                  )}
                >
                  <td className="px-3 py-0.5 text-muted-foreground whitespace-nowrap w-[190px] align-top">
                    {e.timestamp.replace('T', ' ').replace('Z', '')}
                  </td>
                  <td className="px-2 py-0.5 w-[64px] align-top">
                    <Badge
                      variant="outline"
                      className={cn('text-[10px] px-1 py-0 font-bold', LEVEL_STYLES[e.level] ?? '')}
                    >
                      {e.level}
                    </Badge>
                  </td>
                  <td className="px-2 py-0.5 text-muted-foreground/60 w-[180px] align-top truncate max-w-[180px]" title={e.target}>
                    {e.target.split('::').pop()}
                  </td>
                  <td className="px-2 py-0.5 pr-4 break-all align-top">
                    {e.message}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <div ref={bottomRef} />
      </div>
      </div>

      {paused && (
        <div className="text-center text-xs text-yellow-600 dark:text-yellow-400 py-1 bg-yellow-500/10 border-t">
          Pausiert — keine neuen Einträge werden geladen
        </div>
      )}
    </div>
  );
}

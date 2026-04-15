import { useState, useEffect } from 'react';
import { getInstanceHistory, type HistoryEntry, type HistoryQuery } from '../lib/tauri';
import { 
  Play, CheckCircle, Activity, Settings as SettingsIcon, 
  XCircle, Filter, Camera, ArrowRightCircle
} from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';
import { DataViewer } from '@/shared/components/DataViewer';

interface HistoryTimelineProps {
  instanceId: string;
  refreshTrigger?: number;
}

export function HistoryTimeline({ instanceId, refreshTrigger = 0 }: HistoryTimelineProps) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Filters
  const [eventTypes, setEventTypes] = useState<string>('');
  const [actorTypes, setActorTypes] = useState<string>('');

  // Dialog State
  const [selectedEntry, setSelectedEntry] = useState<HistoryEntry | null>(null);

  const fetchHistory = async () => {
    setLoading(true);
    setError(null);
    try {
      const query: HistoryQuery = {};
      if (eventTypes) query.event_types = eventTypes;
      if (actorTypes) query.actor_types = actorTypes;

      const result = await getInstanceHistory(instanceId, query);
      setEntries(result);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId, eventTypes, actorTypes, refreshTrigger]);

  const getEventIcon = (type: string) => {
    switch (type) {
      case 'InstanceStarted': return <Play className="h-4 w-4 text-blue-500" />;
      case 'InstanceCompleted': return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'InstanceFailed': return <XCircle className="h-4 w-4 text-destructive" />;
      case 'TokenAdvanced': return <ArrowRightCircle className="h-4 w-4 text-muted-foreground" />;
      case 'VariablesChanged': return <SettingsIcon className="h-4 w-4 text-indigo-500" />;
      default: return <Activity className="h-4 w-4 text-muted-foreground" />;
    }
  };

  const getActorVariant = (type?: string): "default" | "secondary" | "outline" | "destructive" => {
    if (!type) return 'outline';
    switch (type.toLowerCase()) {
      case 'engine': return 'default';
      case 'serviceworker': return 'secondary';
      case 'user': return 'default'; // Maybe distinct class later
      case 'timer': return 'outline';
      case 'api': return 'secondary';
      default: return 'outline';
    }
  };

  const getActorCustomClass = (type?: string) => {
    if (!type) return '';
    switch (type.toLowerCase()) {
      case 'engine': return 'bg-purple-100 text-purple-700 hover:bg-purple-100 dark:bg-purple-900/40 dark:text-purple-400';
      case 'serviceworker': return 'bg-orange-100 text-orange-700 hover:bg-orange-100 dark:bg-orange-900/40 dark:text-orange-400';
      case 'user': return 'bg-cyan-100 text-cyan-700 hover:bg-cyan-100 dark:bg-cyan-900/40 dark:text-cyan-400';
      case 'timer': return 'bg-amber-100 text-amber-700 hover:bg-amber-100 dark:bg-amber-900/40 dark:text-amber-400';
      case 'api': return 'bg-emerald-100 text-emerald-700 hover:bg-emerald-100 dark:bg-emerald-900/40 dark:text-emerald-400';
      default: return '';
    }
  };

  return (
    <div className="history-timeline-container flex flex-col gap-4">
      {/* Filters */}
      <div className="flex flex-wrap items-center gap-3 p-3 bg-muted/40 rounded-md border">
         <div className="flex items-center gap-2 text-muted-foreground">
           <Filter className="h-4 w-4" />
           <span className="text-sm font-semibold text-foreground">Filters</span>
         </div>
         <select 
            value={eventTypes} 
            onChange={e => setEventTypes(e.target.value)}
            className="h-9 rounded-md border border-input bg-background text-foreground px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
         >
           <option value="">All Events</option>
           <option value="InstanceStarted,InstanceCompleted,InstanceFailed">Lifecycle Only</option>
           <option value="TokenAdvanced">Token Movements</option>
           <option value="VariablesChanged">Variable Changes</option>
         </select>

         <select 
            value={actorTypes} 
            onChange={e => setActorTypes(e.target.value)}
            className="h-9 rounded-md border border-input bg-background text-foreground px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
         >
           <option value="">All Actors</option>
           <option value="engine">Engine</option>
           <option value="serviceworker">Service Worker</option>
           <option value="user">User</option>
           <option value="api">API</option>
         </select>
      </div>

      {loading && <div className="text-sm text-muted-foreground py-2">Loading history...</div>}
      {error && <div className="text-sm text-destructive font-medium py-2">Error: {error}</div>}

      {!loading && entries.length === 0 && (
        <div className="text-sm text-muted-foreground py-4 text-center border rounded-md bg-muted/20">No history entries found for the current filters.</div>
      )}

      {/* Compact List View */}
      {entries.length > 0 && (
        <div className="border rounded-md overflow-hidden bg-background">
          <Table>
            <TableHeader className="bg-muted/40 hover:bg-muted/40">
              <TableRow>
                <TableHead className="w-10 text-center"></TableHead>
                <TableHead>Action</TableHead>
                <TableHead>Who</TableHead>
                <TableHead>When</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {entries.map((entry) => (
                <TableRow 
                  key={entry.id} 
                  onClick={() => setSelectedEntry(entry)}
                  className="cursor-pointer hover:bg-muted/50 transition-colors"
                >
                  <TableCell className="text-center py-2 h-10 w-10">
                    <div className="flex justify-center items-center w-full h-full"> 
                      {getEventIcon(entry.event_type)}
                    </div>
                  </TableCell>
                  <TableCell className="font-medium">
                    <div className="flex items-center gap-2" title="Snapshot details">
                      {(entry.event_type || 'Unknown').replace(/([A-Z])/g, ' $1').trim()}
                      {entry.is_snapshot && <Camera className="h-3 w-3 text-muted-foreground" />}
                    </div>
                  </TableCell>
                  <TableCell>
                    <Badge variant={getActorVariant(entry.actor_type)} className={cn("lowercase capitalize", getActorCustomClass(entry.actor_type))}>
                      {entry.actor_type || 'Unknown'}{entry.actor_id ? ` (${entry.actor_id})` : ''}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-muted-foreground text-xs">
                    {new Date(entry.timestamp).toLocaleString()}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      {/* Detail Dialog */}
      <Dialog open={!!selectedEntry} onOpenChange={(open) => !open && setSelectedEntry(null)}>
        <DialogContent className="sm:max-w-[600px] h-fit max-h-[90vh] flex flex-col p-0">
          <DialogHeader className="px-6 py-4 border-b">
            <DialogTitle className="flex items-center gap-2 text-xl font-semibold">
              {selectedEntry && getEventIcon(selectedEntry.event_type)}
              {(selectedEntry?.event_type || 'Unknown').replace(/([A-Z])/g, ' $1').trim()}
            </DialogTitle>
          </DialogHeader>

          {selectedEntry && (
            <div className="flex-1 p-6 overflow-y-auto min-h-0 relative">
              <div className="space-y-6">
                <div className="flex flex-wrap gap-3">
                  <Badge variant={getActorVariant(selectedEntry.actor_type)} className={cn("lowercase capitalize", getActorCustomClass(selectedEntry.actor_type))}>
                    {selectedEntry.actor_type || 'Unknown'}{selectedEntry.actor_id ? ` (${selectedEntry.actor_id})` : ''}
                  </Badge>
                  <span className="text-sm text-muted-foreground flex items-center">
                    {new Date(selectedEntry.timestamp).toLocaleString()}
                  </span>
                  {selectedEntry.node_id && (
                    <Badge variant="outline" className="font-mono bg-muted/30">
                      Node: {selectedEntry.node_id}
                    </Badge>
                  )}
                </div>

                <div className="p-3 bg-muted/40 rounded-md border text-sm text-foreground">
                  {selectedEntry.description}
                </div>

                {selectedEntry.diff?.human_readable && (
                  <div>
                    <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">Changes</h3>
                    <div className="p-3 bg-muted/20 border-l-4 border-l-primary/50 text-sm whitespace-pre-wrap font-mono">
                      {selectedEntry.diff.human_readable}
                    </div>
                  </div>
                )}

                {selectedEntry.diff?.changes && Object.keys(selectedEntry.diff.changes).length > 0 && (
                  <div>
                    <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">Raw Data Changes</h3>
                    <DataViewer
                      content={JSON.stringify(selectedEntry.diff.changes, null, 2)}
                      format="json"
                      height="200px"
                    />
                  </div>
                )}
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

import { useState, useCallback, useEffect, useRef } from 'react';
import {
  queryCompletedInstances,
  listDefinitions,
  type ProcessInstance,
  type DefinitionInfo,
  type CompletedInstanceQuery,
} from '../../shared/lib/tauri';
import { usePolling } from '../../shared/hooks/use-polling';
import { useEngineEvents } from '../../shared/hooks/use-engine-events';
import { useToast } from '@/hooks/use-toast';
import { History, RefreshCw, Search, ExternalLink } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { PageHeader } from '../../shared/components/PageHeader';
import { EmptyState } from '../../shared/components/EmptyState';

const PAGE_SIZE = 50;

function formatDateTime(dateStr: string | null | undefined): string {
  if (!dateStr) return '-';
  return new Date(dateStr).toLocaleString();
}

function formatDuration(startedAt: string | null | undefined, completedAt: string | null | undefined): string {
  if (!startedAt || !completedAt) return '-';
  const ms = new Date(completedAt).getTime() - new Date(startedAt).getTime();
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  if (ms < 3_600_000) return `${Math.round(ms / 60_000)}min`;
  return `${(ms / 3_600_000).toFixed(1)}h`;
}

function instanceStateBadge(state: ProcessInstance['state']): { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' } {
  if (state === 'Completed') return { label: 'Completed', variant: 'default' };
  if (state === 'Running') return { label: 'Running', variant: 'secondary' };
  if (typeof state === 'object') {
    if ('CompletedWithError' in state || 'ErrorEnd' in state) return { label: 'Error', variant: 'destructive' };
  }
  return { label: String(state), variant: 'outline' };
}

export function HistoryPage({ onViewInstance }: { onViewInstance?: (id: string) => void }) {
  const { toast } = useToast();
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(false);

  // Filter state
  const [businessKey, setBusinessKey] = useState('');
  const [definitionKey, setDefinitionKey] = useState('');
  const [stateFilter, setStateFilter] = useState('');

  const buildQuery = useCallback((): CompletedInstanceQuery => {
    const q: CompletedInstanceQuery = { limit: PAGE_SIZE, offset };
    if (businessKey.trim()) q.business_key = businessKey.trim();
    if (definitionKey) q.definition_key = definitionKey;
    if (stateFilter) q.state_filter = stateFilter as 'completed' | 'error';
    return q;
  }, [businessKey, definitionKey, stateFilter, offset]);

  const fetchData = useCallback(async () => {
    try {
      const [results, defs] = await Promise.all([
        queryCompletedInstances(buildQuery()),
        definitions.length === 0 ? listDefinitions() : Promise.resolve(definitions),
      ]);
      setInstances(results);
      setHasMore(results.length >= PAGE_SIZE);
      if (definitions.length === 0) setDefinitions(defs);
      setLoading(false);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Failed to load history: ' + e });
    }
  }, [buildQuery, definitions, toast]);

  usePolling(fetchData, 30000);
  useEngineEvents(fetchData, ['instance_changed']);

  // Refetch whenever the offset changes (pagination), after the initial mount.
  const didMountRef = useRef(false);
  useEffect(() => {
    if (!didMountRef.current) { didMountRef.current = true; return; }
    fetchData();
  }, [offset]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSearch = () => {
    setOffset(0);
    setLoading(true);
    fetchData();
  };

  const defMap = new Map(definitions.map(d => [d.key, d]));

  return (
    <div className="flex flex-col h-full bg-background">
      <PageHeader
        title="History"
        icon={<History className="h-6 w-6 text-primary" />}
        actions={
          <Button onClick={fetchData} variant="outline" size="sm" className="gap-2">
            <RefreshCw className="h-4 w-4" /> Refresh
          </Button>
        }
      />

      {/* Filter bar */}
      <div className="px-6 py-4 border-b flex flex-wrap items-end gap-3">
        <div className="flex flex-col gap-1">
          <label className="text-xs text-muted-foreground">Business Key</label>
          <Input
            placeholder="Search..."
            value={businessKey}
            onChange={e => setBusinessKey(e.target.value)}
            className="w-[200px] h-9"
            onKeyDown={e => e.key === 'Enter' && handleSearch()}
          />
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-xs text-muted-foreground">Definition</label>
          <select
            value={definitionKey}
            onChange={e => setDefinitionKey(e.target.value)}
            className="w-[200px] h-9 rounded-md border border-input bg-background px-3 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <option value="">All</option>
            {definitions.filter(d => d.is_latest).map(d => (
              <option key={d.key} value={d.key}>
                {d.bpmn_id} (v{d.version})
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-xs text-muted-foreground">Status</label>
          <select
            value={stateFilter}
            onChange={e => setStateFilter(e.target.value)}
            className="w-[140px] h-9 rounded-md border border-input bg-background px-3 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <option value="">All</option>
            <option value="completed">Completed</option>
            <option value="error">Error</option>
          </select>
        </div>

        <Button onClick={handleSearch} size="sm" className="gap-2 h-9">
          <Search className="h-4 w-4" /> Search
        </Button>
      </div>

      {/* Results table */}
      <ScrollArea className="flex-1">
        <div className="p-6">
          {loading && instances.length === 0 && (
            <div className="space-y-3">
              {[1, 2, 3, 4, 5].map(i => (
                <Skeleton key={i} className="h-12 w-full" />
              ))}
            </div>
          )}

          {!loading && instances.length === 0 && (
            <EmptyState
              icon={History}
              title="No Historical Instances"
              description="No completed process instances found matching your filters."
            />
          )}

          {instances.length > 0 && (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Instance ID</TableHead>
                    <TableHead>Business Key</TableHead>
                    <TableHead>Definition</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Started</TableHead>
                    <TableHead>Completed</TableHead>
                    <TableHead>Duration</TableHead>
                    <TableHead className="w-[50px]" />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {instances.map(inst => {
                    const def = defMap.get(inst.definition_key);
                    const badge = instanceStateBadge(inst.state);
                    return (
                      <TableRow
                        key={inst.id}
                        className="cursor-pointer hover:bg-muted/50"
                        onClick={() => onViewInstance?.(inst.id)}
                      >
                        <TableCell className="font-mono text-xs">
                          {inst.id.substring(0, 8)}
                        </TableCell>
                        <TableCell>{inst.business_key || '-'}</TableCell>
                        <TableCell className="font-mono text-xs">
                          {def ? `${def.bpmn_id} v${def.version}` : inst.definition_key.substring(0, 8)}
                        </TableCell>
                        <TableCell>
                          <Badge variant={badge.variant}>{badge.label}</Badge>
                        </TableCell>
                        <TableCell className="text-xs">{formatDateTime(inst.started_at)}</TableCell>
                        <TableCell className="text-xs">{formatDateTime(inst.completed_at)}</TableCell>
                        <TableCell className="text-xs font-mono">
                          {formatDuration(inst.started_at, inst.completed_at)}
                        </TableCell>
                        <TableCell>
                          {onViewInstance && (
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-7 w-7"
                              onClick={e => { e.stopPropagation(); onViewInstance(inst.id); }}
                            >
                              <ExternalLink className="h-3.5 w-3.5" />
                            </Button>
                          )}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>

              {/* Pagination */}
              <div className="flex items-center justify-between mt-4">
                <span className="text-sm text-muted-foreground">
                  Showing {offset + 1}–{offset + instances.length}
                </span>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={offset === 0}
                    onClick={() => { setOffset(Math.max(0, offset - PAGE_SIZE)); setLoading(true); }}
                  >
                    Previous
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={!hasMore}
                    onClick={() => { setOffset(offset + PAGE_SIZE); setLoading(true); }}
                  >
                    Next
                  </Button>
                </div>
              </div>
            </>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

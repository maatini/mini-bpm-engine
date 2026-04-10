import { useState, useCallback } from 'react';
import { getPendingServiceTasks, retryIncident, type PendingServiceTask } from '../../shared/lib/tauri';
import { usePolling } from '../../shared/hooks/use-polling';
import { IncidentDetailDialog } from './IncidentDetailDialog';
import { useToast } from '@/hooks/use-toast';
import { AlertTriangle, RefreshCw, ExternalLink, RotateCcw, Search } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHeader } from '../../shared/components/PageHeader';
import { EmptyState } from '../../shared/components/EmptyState';

export function IncidentsPage({ onViewInstance }: { onViewInstance?: (id: string) => void }) {
  const { toast } = useToast();
  const [incidents, setIncidents] = useState<PendingServiceTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedIncident, setSelectedIncident] = useState<PendingServiceTask | null>(null);
  const [retryingId, setRetryingId] = useState<string | null>(null);

  const fetchIncidents = useCallback(async () => {
    try {
      const all = await getPendingServiceTasks();
      setIncidents(all.filter(t => t.retries <= 0));
      setLoading(false);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Failed to load incidents: ' + e });
    }
  }, [toast]);

  usePolling(fetchIncidents, 5000, !selectedIncident);

  const handleQuickRetry = async (e: React.MouseEvent, inc: PendingServiceTask) => {
    e.stopPropagation();
    setRetryingId(inc.id);
    try {
      await retryIncident(inc.id, 3);
      toast({ description: `Incident on '${inc.node_id}' retried.` });
      fetchIncidents();
    } catch (err: any) {
      toast({ variant: 'destructive', description: 'Retry failed: ' + err });
    } finally {
      setRetryingId(null);
    }
  };

  const handleResolved = () => {
    setSelectedIncident(null);
    fetchIncidents();
  };

  return (
    <div className="flex flex-col h-full bg-background">
      <PageHeader
        title="Incidents"
        icon={<AlertTriangle className="h-6 w-6 text-destructive" />}
        actions={
          <>
            {incidents.length > 0 && (
              <Badge variant="destructive" className="text-sm">{incidents.length} active</Badge>
            )}
            <span className="text-xs text-muted-foreground bg-muted px-2 py-1 rounded-md">Auto-refreshing</span>
            <Button onClick={fetchIncidents} variant="outline" size="sm" className="gap-2">
              <RefreshCw className="h-4 w-4" /> Refresh
            </Button>
          </>
        }
      />

      <ScrollArea className="flex-1 w-full h-[calc(100vh-73px)]">
        <div className="p-6">
          {loading && incidents.length === 0 && (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
              {[1,2,3,4].map(i => (
                <Card key={i} className="flex flex-col h-[200px]">
                  <div className="p-4 flex-1 space-y-4">
                    <Skeleton className="h-6 w-[140px]" />
                    <Skeleton className="h-4 w-full" />
                    <Skeleton className="h-10 w-full" />
                  </div>
                </Card>
              ))}
            </div>
          )}

          {!loading && incidents.length === 0 && (
            <EmptyState
              icon={AlertTriangle}
              title="No Incidents"
              description="All systems operational — no failed service tasks."
            />
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
            {incidents.map(inc => (
              <Card
                key={inc.id}
                className="border-destructive/50 flex flex-col cursor-pointer hover:border-destructive/80 transition-colors"
                onClick={() => setSelectedIncident(inc)}
              >
                <CardHeader className="pb-3 border-b bg-destructive/5">
                  <CardTitle className="text-lg flex items-center gap-2 text-destructive font-mono">
                    <AlertTriangle className="h-5 w-5" /> {inc.node_id}
                  </CardTitle>
                </CardHeader>
                <CardContent className="pt-4 space-y-4 flex-1">
                  <div className="flex flex-col gap-2 text-sm">
                    <div className="flex justify-between items-center">
                      <span className="text-muted-foreground">Topic</span>
                      <Badge variant="outline" className="font-mono">{inc.topic}</Badge>
                    </div>
                    <div className="flex justify-between items-center">
                      <span className="text-muted-foreground">Instance</span>
                      <span className="font-mono bg-muted px-2 py-0.5 rounded text-xs">{inc.instance_id.substring(0, 8)}</span>
                    </div>
                  </div>

                  <div className="bg-destructive/10 p-3 rounded-md border border-destructive/20 text-sm">
                    <strong className="text-destructive block mb-1">Error:</strong>
                    <span className="text-foreground line-clamp-3">{inc.error_message || 'No error message'}</span>
                  </div>
                </CardContent>
                <CardFooter className="pt-2 flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="flex-1 gap-1.5"
                    disabled={retryingId === inc.id}
                    onClick={(e) => handleQuickRetry(e, inc)}
                  >
                    <RotateCcw className="h-3.5 w-3.5" /> Retry
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="flex-1 gap-1.5"
                    onClick={(e) => { e.stopPropagation(); setSelectedIncident(inc); }}
                  >
                    <Search className="h-3.5 w-3.5" /> Details
                  </Button>
                  {onViewInstance && (
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 shrink-0"
                      onClick={(e) => { e.stopPropagation(); onViewInstance(inc.instance_id); }}
                    >
                      <ExternalLink className="h-3.5 w-3.5" />
                    </Button>
                  )}
                </CardFooter>
              </Card>
            ))}
          </div>
        </div>
      </ScrollArea>

      <IncidentDetailDialog
        incident={selectedIncident}
        onClose={() => setSelectedIncident(null)}
        onResolved={handleResolved}
        onViewInstance={onViewInstance}
      />
    </div>
  );
}

import { useState, useCallback } from 'react';
import {
  getPendingTimers,
  getPendingServiceTasks,
  getPendingMessageCatches,
  type PendingTimer,
  type PendingServiceTask,
  type PendingMessageCatch
} from '../../shared/lib/tauri';
import { usePolling } from '../../shared/hooks/use-polling';
import { useToast } from '@/hooks/use-toast';
import { Eye, RefreshCw, Timer, Mail, Briefcase, ExternalLink } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { PageHeader } from '../../shared/components/PageHeader';
import { EmptyState } from '../../shared/components/EmptyState';

function formatRelativeTime(dateStr: string): string {
  const now = Date.now();
  const target = new Date(dateStr).getTime();
  const diff = target - now;
  const absDiff = Math.abs(diff);

  if (absDiff < 60_000) return diff > 0 ? 'in < 1 min' : '< 1 min ago';
  if (absDiff < 3_600_000) {
    const mins = Math.round(absDiff / 60_000);
    return diff > 0 ? `in ${mins} min` : `${mins} min ago`;
  }
  if (absDiff < 86_400_000) {
    const hours = Math.round(absDiff / 3_600_000);
    return diff > 0 ? `in ${hours}h` : `${hours}h ago`;
  }
  const days = Math.round(absDiff / 86_400_000);
  return diff > 0 ? `in ${days}d` : `${days}d ago`;
}

function timerTypeBadge(timer: PendingTimer): string {
  if (!timer.timer_def) return 'Timer';
  if ('Date' in timer.timer_def) return 'Date';
  if ('Duration' in timer.timer_def) return 'Duration';
  if ('RepeatingInterval' in timer.timer_def) return 'Repeating';
  return 'Timer';
}

export function OverviewPage({ onViewInstance }: { onViewInstance?: (id: string) => void }) {
  const { toast } = useToast();
  const [timers, setTimers] = useState<PendingTimer[]>([]);
  const [messages, setMessages] = useState<PendingMessageCatch[]>([]);
  const [jobs, setJobs] = useState<PendingServiceTask[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchAll = useCallback(async () => {
    try {
      const [t, m, s] = await Promise.all([
        getPendingTimers(),
        getPendingMessageCatches(),
        getPendingServiceTasks(),
      ]);
      setTimers(t);
      setMessages(m);
      setJobs(s.filter(task => task.retries > 0));
      setLoading(false);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Failed to load overview: ' + e });
    }
  }, [toast]);

  usePolling(fetchAll, 5000);

  const skeletonCards = (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
      {[1, 2, 3, 4].map(i => (
        <Card key={i} className="flex flex-col h-[160px]">
          <div className="p-4 flex-1 space-y-4">
            <Skeleton className="h-6 w-[140px]" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
          </div>
        </Card>
      ))}
    </div>
  );

  return (
    <div className="flex flex-col h-full bg-background">
      <PageHeader
        title="Overview"
        icon={<Eye className="h-6 w-6 text-primary" />}
        actions={
          <>
            <span className="text-xs text-muted-foreground bg-muted px-2 py-1 rounded-md">Auto-refreshing</span>
            <Button onClick={fetchAll} variant="outline" size="sm" className="gap-2">
              <RefreshCw className="h-4 w-4" /> Refresh
            </Button>
          </>
        }
      />

      <div className="flex-1 overflow-hidden">
        <Tabs defaultValue="timers" className="flex flex-col h-full">
          <div className="px-6 pt-4">
            <TabsList>
              <TabsTrigger value="timers" className="gap-2">
                <Timer className="h-4 w-4" />
                Timers
                {timers.length > 0 && <Badge variant="secondary" className="ml-1 h-5 px-1.5 text-xs">{timers.length}</Badge>}
              </TabsTrigger>
              <TabsTrigger value="messages" className="gap-2">
                <Mail className="h-4 w-4" />
                Messages
                {messages.length > 0 && <Badge variant="secondary" className="ml-1 h-5 px-1.5 text-xs">{messages.length}</Badge>}
              </TabsTrigger>
              <TabsTrigger value="jobs" className="gap-2">
                <Briefcase className="h-4 w-4" />
                Jobs
                {jobs.length > 0 && <Badge variant="secondary" className="ml-1 h-5 px-1.5 text-xs">{jobs.length}</Badge>}
              </TabsTrigger>
            </TabsList>
          </div>

          {/* Timers Tab */}
          <TabsContent value="timers" className="flex-1 overflow-hidden mt-0">
            <ScrollArea className="h-[calc(100vh-170px)]">
              <div className="p-6">
                {loading && timers.length === 0 && skeletonCards}

                {!loading && timers.length === 0 && (
                  <EmptyState icon={Timer} title="No Active Timers" description="No pending timers across all instances." />
                )}

                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                  {timers.map(timer => (
                    <Card key={timer.id} className="border-blue-500/30 flex flex-col hover:border-blue-500/60 transition-colors">
                      <CardHeader className="pb-3 border-b bg-blue-500/5">
                        <CardTitle className="text-lg flex items-center gap-2 font-mono">
                          <Timer className="h-5 w-5 text-blue-500" /> {timer.node_id}
                        </CardTitle>
                      </CardHeader>
                      <CardContent className="pt-4 space-y-3 flex-1">
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Type</span>
                          <Badge variant="outline">{timerTypeBadge(timer)}</Badge>
                        </div>
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Expires</span>
                          <span className="font-mono text-xs" title={timer.expires_at}>
                            {formatRelativeTime(timer.expires_at)}
                          </span>
                        </div>
                        {timer.remaining_repetitions != null && (
                          <div className="flex justify-between items-center text-sm">
                            <span className="text-muted-foreground">Remaining</span>
                            <span className="font-mono">{timer.remaining_repetitions}</span>
                          </div>
                        )}
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Instance</span>
                          <div className="flex items-center gap-1">
                            <span className="font-mono bg-muted px-2 py-0.5 rounded text-xs">{timer.instance_id.substring(0, 8)}</span>
                            {onViewInstance && (
                              <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => onViewInstance(timer.instance_id)}>
                                <ExternalLink className="h-3 w-3" />
                              </Button>
                            )}
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </div>
            </ScrollArea>
          </TabsContent>

          {/* Messages Tab */}
          <TabsContent value="messages" className="flex-1 overflow-hidden mt-0">
            <ScrollArea className="h-[calc(100vh-170px)]">
              <div className="p-6">
                {loading && messages.length === 0 && skeletonCards}

                {!loading && messages.length === 0 && (
                  <EmptyState icon={Mail} title="No Pending Messages" description="No message catch events waiting for correlation." />
                )}

                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                  {messages.map(msg => (
                    <Card key={msg.id} className="border-amber-500/30 flex flex-col hover:border-amber-500/60 transition-colors">
                      <CardHeader className="pb-3 border-b bg-amber-500/5">
                        <CardTitle className="text-lg flex items-center gap-2 font-mono">
                          <Mail className="h-5 w-5 text-amber-500" /> {msg.message_name}
                        </CardTitle>
                      </CardHeader>
                      <CardContent className="pt-4 space-y-3 flex-1">
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Node</span>
                          <Badge variant="outline" className="font-mono">{msg.node_id}</Badge>
                        </div>
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Instance</span>
                          <div className="flex items-center gap-1">
                            <span className="font-mono bg-muted px-2 py-0.5 rounded text-xs">{msg.instance_id.substring(0, 8)}</span>
                            {onViewInstance && (
                              <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => onViewInstance(msg.instance_id)}>
                                <ExternalLink className="h-3 w-3" />
                              </Button>
                            )}
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </div>
            </ScrollArea>
          </TabsContent>

          {/* Jobs Tab */}
          <TabsContent value="jobs" className="flex-1 overflow-hidden mt-0">
            <ScrollArea className="h-[calc(100vh-170px)]">
              <div className="p-6">
                {loading && jobs.length === 0 && skeletonCards}

                {!loading && jobs.length === 0 && (
                  <EmptyState icon={Briefcase} title="No Active Jobs" description="No service task jobs currently in progress." />
                )}

                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                  {jobs.map(job => (
                    <Card key={job.id} className="border-green-500/30 flex flex-col hover:border-green-500/60 transition-colors">
                      <CardHeader className="pb-3 border-b bg-green-500/5">
                        <CardTitle className="text-lg flex items-center gap-2 font-mono">
                          <Briefcase className="h-5 w-5 text-green-500" /> {job.node_id}
                        </CardTitle>
                      </CardHeader>
                      <CardContent className="pt-4 space-y-3 flex-1">
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Topic</span>
                          <Badge variant="outline" className="font-mono">{job.topic}</Badge>
                        </div>
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Worker</span>
                          <span className="font-mono text-xs">
                            {job.worker_id ? (
                              <Badge variant="secondary">{job.worker_id}</Badge>
                            ) : (
                              <span className="text-muted-foreground">unlocked</span>
                            )}
                          </span>
                        </div>
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Retries</span>
                          <span className="font-mono">{job.retries}</span>
                        </div>
                        <div className="flex justify-between items-center text-sm">
                          <span className="text-muted-foreground">Instance</span>
                          <div className="flex items-center gap-1">
                            <span className="font-mono bg-muted px-2 py-0.5 rounded text-xs">{job.instance_id.substring(0, 8)}</span>
                            {onViewInstance && (
                              <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => onViewInstance(job.instance_id)}>
                                <ExternalLink className="h-3 w-3" />
                              </Button>
                            )}
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </div>
            </ScrollArea>
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

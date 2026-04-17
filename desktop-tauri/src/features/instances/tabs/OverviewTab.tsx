import { useMemo } from 'react';
import { Clock, User, Wrench, Timer } from 'lucide-react';
import { type ProcessInstance, type DefinitionInfo, type PendingUserTask, type PendingServiceTask } from '../../../shared/types/engine';
import { ErrorBoundary } from '../../../shared/components/ErrorBoundary';
import { InstanceViewer } from '../InstanceViewer';
import { stateBadgeClass, stateLabel } from '../InstanceStateUtils';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import { cn } from '@/lib/utils';

interface OverviewTabProps {
  instance: ProcessInstance;
  liveTokens: ProcessInstance['tokens'];
  definitionXml: string | null;
  defMap: Map<string, DefinitionInfo>;
  pendingTasks: PendingUserTask[];
  pendingServiceTasks: PendingServiceTask[];
  onNodeClick: () => void;
}

export function OverviewTab({
  instance,
  liveTokens,
  definitionXml,
  defMap,
  pendingTasks,
  pendingServiceTasks,
  onNodeClick,
}: OverviewTabProps) {
  const def = defMap.get(instance.definition_key);

  const activeNodeIds = useMemo(() => {
    const tokens = liveTokens ?? instance.tokens;
    if (tokens && Object.keys(tokens).length > 0) {
      return [...new Set(
        Object.values(tokens)
          .filter(t => !t.is_merged)
          .map(t => t.current_node)
          .filter(Boolean)
      )];
    }
    return [instance.current_node].filter(Boolean) as string[];
  }, [liveTokens, instance.current_node, instance.tokens]);

  const activeTokens = useMemo(() => {
    const tokens = liveTokens ?? instance.tokens;
    if (!tokens) return [];
    return Object.entries(tokens)
      .filter(([, t]) => !t.is_merged)
      .map(([id, t]) => ({ id, ...t }));
  }, [liveTokens, instance.tokens]);

  const timerStartNodeId =
    instance.variables._timer_start_node &&
    typeof instance.variables._timer_iteration === 'number' &&
    typeof instance.variables._timer_total === 'number' &&
    instance.variables._timer_iteration < (instance.variables._timer_total as number)
      ? String(instance.variables._timer_start_node)
      : undefined;

  return (
    <div className="p-6 space-y-6">
      {/* Meta-Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card className="p-4 flex flex-col gap-1.5 shadow-sm">
          <span className="text-xs uppercase font-semibold text-muted-foreground">Status</span>
          <Badge className={cn('w-fit border-none', stateBadgeClass(instance.state))}>
            {stateLabel(instance.state)}
          </Badge>
        </Card>
        <Card className="p-4 flex flex-col gap-1.5 shadow-sm">
          <span className="text-xs uppercase font-semibold text-muted-foreground">Business Key</span>
          <span className="font-semibold text-base">{instance.business_key || '—'}</span>
        </Card>
        <Card className="p-4 flex flex-col gap-1.5 shadow-sm">
          <span className="text-xs uppercase font-semibold text-muted-foreground">Prozess</span>
          <div className="flex items-center gap-2">
            <span className="font-mono font-semibold">
              {def?.bpmn_id || instance.definition_key.substring(0, 8)}
            </span>
            {def && <Badge variant="outline">v{def.version}</Badge>}
          </div>
        </Card>
      </div>

      {/* Active Tokens */}
      {activeTokens.length > 0 && (
        <div className="space-y-3">
          <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
            Aktive Tokens ({activeTokens.length})
          </h3>
          <div className="flex flex-wrap gap-3">
            {activeTokens.map(token => (
              <div
                key={token.id}
                className="relative flex items-center gap-3 px-4 py-3 rounded-xl border-2 border-emerald-500/60 bg-emerald-500/5 dark:bg-emerald-500/10 shadow-sm min-w-[180px]"
              >
                {/* Pulse ring */}
                <span className="absolute -inset-[3px] rounded-xl border-2 border-emerald-400/40 animate-[token-pulse_2s_ease-in-out_infinite]" />
                <div className="relative flex flex-col gap-0.5">
                  <span className="text-xs text-muted-foreground font-mono">
                    #{token.id.substring(0, 6)}
                  </span>
                  <span className="font-semibold text-sm text-emerald-700 dark:text-emerald-400">
                    {token.current_node}
                  </span>
                  {Object.keys(token.variables || {}).length > 0 && (
                    <span className="text-xs text-muted-foreground">
                      {Object.keys(token.variables).length} Variablen
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* BPMN Viewer */}
      {definitionXml && (
        <div className="space-y-3">
          <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide">
            Prozess-Diagramm
          </h3>
          <ErrorBoundary>
            <div className="border rounded-lg bg-card overflow-hidden h-[380px]">
              <InstanceViewer
                xml={definitionXml}
                activeNodeIds={activeNodeIds}
                onNodeClick={onNodeClick}
                timerStartNodeId={timerStartNodeId}
              />
            </div>
          </ErrorBoundary>
          {timerStartNodeId && (
            <div className="flex items-center gap-2 text-sm px-3 py-2 rounded-md border bg-amber-50 border-amber-300 text-amber-800 dark:bg-amber-950/30 dark:border-amber-800 dark:text-amber-300">
              <Timer className="h-4 w-4 shrink-0" />
              <span>
                Timer-Zyklus: {instance.variables._timer_iteration as number} von {instance.variables._timer_total as number}
                {typeof instance.variables._timer_interval_secs === 'number' && (
                  <> (alle {instance.variables._timer_interval_secs}s)</>
                )}
              </span>
            </div>
          )}
        </div>
      )}

      {/* Pending Tasks */}
      {pendingTasks.length > 0 && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide flex items-center gap-2">
            <User className="h-4 w-4" /> User Tasks
          </h3>
          <div className="space-y-2">
            {pendingTasks.map(task => (
              <Card key={task.task_id} className="p-4 flex flex-col gap-1 text-sm border-amber-200 dark:border-amber-800">
                <span className="font-medium">{task.node_id}</span>
                <span className="text-muted-foreground">
                  Zugewiesen: <span className="text-foreground font-medium">{task.assignee || 'Nicht zugewiesen'}</span>
                </span>
              </Card>
            ))}
          </div>
        </div>
      )}

      {pendingServiceTasks.length > 0 && (
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wide flex items-center gap-2">
            <Wrench className="h-4 w-4" /> Service Tasks
          </h3>
          <div className="space-y-2">
            {pendingServiceTasks.map((task, i) => (
              <Card key={task?.id || i} className="p-4 flex flex-col gap-1 text-sm border-purple-200 dark:border-purple-800">
                <div className="flex items-center gap-2">
                  <span className="font-medium">{task?.node_id}</span>
                  <Badge variant="secondary" className="font-mono">{task?.topic}</Badge>
                </div>
                <span className="text-muted-foreground text-xs">
                  Worker: {task?.worker_id || 'Ungesperrt'} · Versuche: {task?.retries}
                </span>
              </Card>
            ))}
          </div>
        </div>
      )}

      {/* Started/Completed timestamps */}
      {(instance.started_at || instance.completed_at) && (
        <div className="flex gap-6 text-xs text-muted-foreground border-t pt-4">
          {instance.started_at && (
            <span className="flex items-center gap-1.5">
              <Clock className="h-3 w-3" /> Gestartet: {new Date(instance.started_at).toLocaleString('de-DE')}
            </span>
          )}
          {instance.completed_at && (
            <span className="flex items-center gap-1.5">
              <Clock className="h-3 w-3" /> Abgeschlossen: {new Date(instance.completed_at).toLocaleString('de-DE')}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

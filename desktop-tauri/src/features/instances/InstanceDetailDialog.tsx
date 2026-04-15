import { useState, useEffect, useCallback, useMemo } from 'react';
import { useEngineEvents } from '../../shared/hooks/use-engine-events';
import { Trash, RefreshCw, Clock, Pause, Play, ArrowRightLeft } from 'lucide-react';
import { type ProcessInstance, type DefinitionInfo, type PendingUserTask, type PendingServiceTask } from '../../shared/types/engine';
import { getInstanceDetails, getDefinitionXml, getPendingTasks, getPendingServiceTasks, updateInstanceVariables, suspendInstance, resumeInstance } from '../../shared/lib/tauri';
import { ErrorBoundary } from '../../shared/components/ErrorBoundary';
import { InstanceViewer } from './InstanceViewer';
import { TokenMoveDialog } from './TokenMoveDialog';
import { HistoryTimeline } from '../../shared/components/HistoryTimeline';
import { VariableEditor, type VariableRow, parseVariables, serializeVariables } from '../../shared/components/VariableEditor';
import { stateBadgeClass, stateLabel } from './InstanceStateUtils';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { cn } from '@/lib/utils';

export function InstanceDetailDialog({ 
  instance, 
  onClose,
  onDeleteRequest,
  defMap
}: { 
  instance: ProcessInstance | null;
  onClose: () => void;
  onDeleteRequest: (id: string) => void;
  defMap: Map<string, DefinitionInfo>;
}) {
  const { toast } = useToast();
  const [detailLoading, setDetailLoading] = useState(false);
  const [selected, setSelected] = useState<ProcessInstance | null>(null);

  const [pendingTasks, setPendingTasks] = useState<PendingUserTask[]>([]);
  const [pendingServiceTasks, setPendingServiceTasks] = useState<PendingServiceTask[]>([]);
  const [variables, setVariables] = useState<VariableRow[]>([]);
  const [deletedKeys, setDeletedKeys] = useState<Set<string>>(new Set());
  const [historyRefreshTrigger, setHistoryRefreshTrigger] = useState(0);

  // Live state for auto-refresh — only Variables and Workflow tokens update periodically.
  // `selected` stays stable to avoid re-rendering the entire dialog.
  const [liveTokens, setLiveTokens] = useState<ProcessInstance['tokens']>(undefined);

  const [definitionXml, setDefinitionXml] = useState<string | null>(null);
  const [showNodeDetails, setShowNodeDetails] = useState(true);
  const [tokenMoveOpen, setTokenMoveOpen] = useState(false);

  useEffect(() => {
    if (!instance) {
      setSelected(null);
      setPendingTasks([]);
      setPendingServiceTasks([]);
      setDefinitionXml(null);
      setShowNodeDetails(true);
      return;
    }

    // If instance changed, reload details
    if (!selected || instance.id !== selected.id) {
      loadInstanceDetails(instance);
    }
  }, [instance]);

  // Lightweight auto-refresh: only Variables and Workflow tokens update periodically.
  const liveRefresh = useCallback(async () => {
    if (!selected) return;
    try {
      const details = await getInstanceDetails(selected.id);
      setVariables(parseVariables(details.variables));
      setLiveTokens(details.tokens);
    } catch {
      // Silently ignore refresh errors
    }
  }, [selected?.id]);

  useEffect(() => {
    if (!selected) return;
    const interval = setInterval(liveRefresh, 30000);
    return () => clearInterval(interval);
  }, [selected?.id, liveRefresh]);
  useEngineEvents(liveRefresh, ['instance_changed', 'task_changed'], !!selected);

  // Full refresh: re-fetches everything including state, pending tasks, and history.
  const refreshDetails = useCallback(async () => {
    if (!selected) return;
    try {
      const details = await getInstanceDetails(selected.id);
      setSelected(details);
      setVariables(parseVariables(details.variables));
      setLiveTokens(details.tokens);
      if (typeof details.state === 'object') {
        if ('WaitingOnUserTask' in details.state) {
          const tasks = await getPendingTasks();
          setPendingTasks(tasks.filter(t => t.instance_id === details.id));
          setPendingServiceTasks([]);
        } else if ('WaitingOnServiceTask' in details.state) {
          const sTasks = await getPendingServiceTasks();
          setPendingServiceTasks(sTasks.filter(t => t.instance_id === details.id));
          setPendingTasks([]);
        } else {
          setPendingTasks([]);
          setPendingServiceTasks([]);
        }
      } else {
        setPendingTasks([]);
        setPendingServiceTasks([]);
      }
      setHistoryRefreshTrigger(prev => prev + 1);
    } catch {
      // Silently ignore refresh errors
    }
  }, [selected?.id]);

  const loadInstanceDetails = async (inst: ProcessInstance) => {
    setDetailLoading(true);
    setDefinitionXml(null);
    setShowNodeDetails(true);
    setHistoryRefreshTrigger(prev => prev + 1);
    
    try {
      const details = await getInstanceDetails(inst.id);
      setSelected(details);
      setLiveTokens(details.tokens);

      try {
        const xml = await getDefinitionXml(details.definition_key);
        setDefinitionXml(xml);
      } catch (xmlError) {
        console.error("Failed to fetch layout XML:", xmlError);
      }

      setVariables(parseVariables(details.variables));
      if (typeof details.state === 'object') {
        if ('WaitingOnUserTask' in details.state) {
          const tasks = await getPendingTasks();
          setPendingTasks(tasks.filter(t => t.instance_id === details.id));
          setPendingServiceTasks([]);
        } else if ('WaitingOnServiceTask' in details.state) {
          const sTasks = await getPendingServiceTasks();
          setPendingServiceTasks(sTasks.filter(t => t.instance_id === details.id));
          setPendingTasks([]);
        } else {
          setPendingTasks([]);
          setPendingServiceTasks([]);
        }
      } else {
        setPendingTasks([]);
        setPendingServiceTasks([]);
      }
    } catch {
      setSelected(inst);
      setVariables(parseVariables(inst.variables));
      setPendingTasks([]);
      setPendingServiceTasks([]);
    } finally {
      setDetailLoading(false);
    }
  };

  const handleSaveVariables = async () => {
    if (!selected) return;
    const varsToSave = serializeVariables(variables, deletedKeys);
    if (varsToSave === null) {
      toast({ variant: 'destructive', description: 'Invalid variables format (check JSON or Numbers)' });
      return;
    }

    try {
      await updateInstanceVariables(selected.id, varsToSave);
      toast({ description: 'Variables saved successfully.' });
      
      // Refresh
      const updated = await getInstanceDetails(selected.id);
      setSelected(updated);
      setVariables(parseVariables(updated.variables));
      setDeletedKeys(new Set());
      setHistoryRefreshTrigger(prev => prev + 1);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Error saving variables: ' + e });
    }
  };

  const isSuspended = selected && typeof selected.state === 'object' && 'Suspended' in selected.state;
  const isCompleted = selected && (selected.state === 'Completed' || (typeof selected.state === 'object' && 'CompletedWithError' in selected.state));

  const activeNodeIds = useMemo(() => {
    const tokens = liveTokens ?? selected?.tokens;
    if (tokens && Object.keys(tokens).length > 0) {
      return [...new Set(
        Object.values(tokens)
          .filter(t => !t.is_merged)
          .map(t => t.current_node)
          .filter(Boolean)
      )];
    }
    return [selected?.current_node].filter(Boolean) as string[];
  }, [liveTokens, selected?.current_node, selected?.tokens]);

  const handleSuspendResume = async () => {
    if (!selected) return;
    try {
      if (isSuspended) {
        await resumeInstance(selected.id);
        toast({ description: 'Instance resumed.' });
      } else {
        await suspendInstance(selected.id);
        toast({ description: 'Instance suspended.' });
      }
      refreshDetails();
    } catch (e: any) {
      toast({ variant: 'destructive', description: String(e) });
    }
  };

  return (
    <Dialog open={!!instance} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="instance-detail max-w-[70vw] w-full h-[90vh] flex flex-col p-0 overflow-hidden bg-background">
        <DialogHeader className="px-6 py-4 border-b flex flex-row items-center justify-between sticky top-0 bg-background/95 backdrop-blur z-10 shrink-0">
          <DialogTitle className="text-xl">Instance Details: {selected?.id.substring(0, 8) || instance?.id.substring(0, 8)}…</DialogTitle>
          <div className="flex gap-2 items-center !m-0">
            {selected && !isCompleted && (
              <>
                <Button
                  variant="outline"
                  size="sm"
                  className="gap-2"
                  onClick={() => setTokenMoveOpen(true)}
                  disabled={isSuspended as boolean}
                >
                  <ArrowRightLeft className="h-4 w-4" /> Token Move
                </Button>
                <Button
                  variant={isSuspended ? "default" : "outline"}
                  size="sm"
                  className="gap-2"
                  onClick={handleSuspendResume}
                >
                  {isSuspended ? <Play className="h-4 w-4" /> : <Pause className="h-4 w-4" />}
                  {isSuspended ? 'Resume' : 'Suspend'}
                </Button>
              </>
            )}
            <Button variant="outline" size="sm" className="gap-2" onClick={refreshDetails}>
              <RefreshCw className="h-4 w-4" /> Refresh
            </Button>
            <Button variant="destructive" size="sm" className="gap-2" onClick={() => selected && onDeleteRequest(selected.id)}>
              <Trash className="h-4 w-4" /> Delete
            </Button>
            <Button variant="outline" size="sm" onClick={() => onClose()} data-testid="btn-close-details" className="gap-1 shadow-sm">
              Close
            </Button>
          </div>
        </DialogHeader>

        <div className="flex-1 p-6 overflow-y-auto min-h-0 relative">
          {detailLoading || !selected ? (
            <div className="text-center text-muted-foreground py-8">Loading instance context...</div>
          ) : (
            <div className="space-y-8">
              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                <Card className="p-4 flex flex-col gap-1.5 shadow-sm">
                  <span className="text-xs uppercase font-semibold text-muted-foreground">State</span>
                  <Badge className={cn("w-fit border-none", stateBadgeClass(selected.state))}>
                    {stateLabel(selected.state)}
                  </Badge>
                </Card>
                <Card className="p-4 flex flex-col gap-1.5 shadow-sm">
                  <span className="text-xs uppercase font-semibold text-muted-foreground">Business Key</span>
                  <span className="font-semibold text-base">{selected.business_key || 'None'}</span>
                </Card>
                <Card className="p-4 flex flex-col gap-1.5 shadow-sm">
                  <span className="text-xs uppercase font-semibold text-muted-foreground">Process ID</span>
                  <div className="flex items-center gap-2">
                      <span className="font-mono text-base font-semibold">{defMap.get(selected.definition_key)?.bpmn_id || selected.definition_key.substring(0, 8)}</span>
                      {defMap.get(selected.definition_key) && (
                        <Badge variant="outline">v{defMap.get(selected.definition_key)?.version}</Badge>
                      )}
                  </div>
                </Card>
              </div>

              {definitionXml && (
                <div className="space-y-4">
                  <h3 className="text-lg font-semibold border-b pb-2">Process Workflow</h3>
                  <ErrorBoundary>
                    <div className="border rounded-md bg-card overflow-hidden h-[400px]">
                      <InstanceViewer
                        xml={definitionXml}
                        activeNodeIds={activeNodeIds}
                        onNodeClick={() => setShowNodeDetails((prev) => !prev)}
                        timerStartNodeId={
                          selected.variables._timer_start_node &&
                          typeof selected.variables._timer_iteration === 'number' &&
                          typeof selected.variables._timer_total === 'number' &&
                          selected.variables._timer_iteration < selected.variables._timer_total
                            ? String(selected.variables._timer_start_node)
                            : undefined
                        }
                      />
                    </div>
                  </ErrorBoundary>
                  {typeof selected.variables._timer_iteration === 'number' && typeof selected.variables._timer_total === 'number' && selected.variables._timer_total > 1 && (
                    <div className={cn(
                      "flex items-center gap-2 text-sm px-3 py-2 rounded-md border",
                      selected.variables._timer_iteration < selected.variables._timer_total
                        ? "bg-amber-50 border-amber-300 text-amber-800 dark:bg-amber-950/30 dark:border-amber-800 dark:text-amber-300"
                        : "bg-muted border-border text-muted-foreground"
                    )}>
                      <Clock className="h-4 w-4 shrink-0" />
                      <span>
                        Timer cycle: instance {selected.variables._timer_iteration} of {selected.variables._timer_total}
                        {typeof selected.variables._timer_interval_secs === 'number' && (
                          <> (every {selected.variables._timer_interval_secs}s)</>
                        )}
                        {selected.variables._timer_iteration < selected.variables._timer_total
                          ? ' — cycle active'
                          : ' — cycle complete'}
                      </span>
                    </div>
                  )}
                  {!showNodeDetails && (
                    <p className="text-sm text-muted-foreground">
                      {activeNodeIds.length > 1
                        ? `Click on one of the ${activeNodeIds.length} highlighted active nodes to view details.`
                        : `Click on the highlighted active node (${activeNodeIds[0]}) to view variables and state details.`}
                    </p>
                  )}
                </div>
              )}

              {(!definitionXml || showNodeDetails) && (
                <div className="space-y-6">
                  <ErrorBoundary>
                    <div className="bg-muted/30 border rounded-lg p-5">
                      <h3 className="text-lg font-semibold flex items-center gap-2 border-b pb-3 mb-4">
                        Node Context: <code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded">{selected.current_node || 'Unknown'}</code>
                      </h3>

                      <div className="space-y-6">
                        {/* Pending user task info */}
                        {pendingTasks?.length > 0 && (
                          <div className="bg-background border rounded-md p-4">
                            <h4 className="font-semibold text-foreground mb-3">Assigned User Tasks:</h4>
                            <div className="space-y-3">
                              {pendingTasks.map(task => (
                                <div key={task.task_id} className="bg-muted/50 p-3 rounded-md border text-sm">
                                  <div className="font-medium">Node: {task.node_id}</div>
                                  <div className="text-muted-foreground mt-1">Assignee: <span className="font-medium text-foreground">{task.assignee || 'Unassigned'}</span></div>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}

                        {/* Pending service task info */}
                        {pendingServiceTasks?.length > 0 && (
                          <div className="bg-background border rounded-md p-4">
                            <h4 className="font-semibold text-foreground mb-3">Pending Service Tasks (Workers):</h4>
                            <div className="space-y-3">
                              {pendingServiceTasks.map((task, index) => (
                                <div key={task?.id || `fallback-${index}`} className="bg-muted/50 p-3 rounded-md border text-sm">
                                  <div className="font-medium">Node: {task?.node_id}</div>
                                  <div className="mt-1">Topic: <Badge variant="secondary" className="font-mono">{task?.topic}</Badge></div>
                                  <div className="text-muted-foreground mt-2">
                                    Worker: {task?.worker_id || 'Unlocked'} &middot; Retries: {task?.retries}
                                  </div>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}

                        {/* Execution History Timeline */}
                        <div>
                          <h4 className="font-semibold text-foreground mb-3">Execution History:</h4>
                          <div className="bg-background border rounded-md p-4">
                            <HistoryTimeline instanceId={selected.id} refreshTrigger={historyRefreshTrigger} />
                          </div>
                        </div>

                        {/* Editable variables */}
                        <div>
                          <h4 className="font-semibold text-foreground mb-3">Variables:</h4>
                          <div className="bg-background border rounded-md p-4">
                            <VariableEditor
                              variables={variables}
                              onChange={setVariables}
                              readOnlyNames={true}
                              deletedKeys={deletedKeys}
                              onDeletedKeysChange={setDeletedKeys}
                              instanceId={selected.id}
                              onVariablesRefreshRequest={() => loadInstanceDetails(selected)}
                            />
                            <div className="mt-4 pt-4 border-t flex justify-end">
                              <Button onClick={handleSaveVariables}>Save Variables</Button>
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                  </ErrorBoundary>
                </div>
              )}
            </div>
          )}
        </div>
      </DialogContent>

      <TokenMoveDialog
        instance={selected}
        xml={definitionXml}
        open={tokenMoveOpen}
        onClose={() => setTokenMoveOpen(false)}
        onMoved={() => {
          setTokenMoveOpen(false);
          refreshDetails();
        }}
      />
    </Dialog>
  );
}

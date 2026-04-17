import { useState, useEffect, useCallback } from 'react';
import { Unlock } from 'lucide-react';
import { type ProcessInstance, type DefinitionInfo, type PendingUserTask, type PendingServiceTask } from '../../shared/types/engine';
import { getInstanceDetails, getDefinitionXml, getPendingTasks, getPendingServiceTasks, updateInstanceVariables, suspendInstance, resumeInstance } from '../../shared/lib/tauri';
import { parseVariables, serializeVariables, type VariableRow } from '../../shared/components/VariableEditor';
import { useEngineEvents } from '../../shared/hooks/use-engine-events';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { InstanceDetailHeader } from './InstanceDetailHeader';
import { TokenMoveDialog } from './TokenMoveDialog';
import { MigrationDialog } from './MigrationDialog';
import { OverviewTab } from './tabs/OverviewTab';
import { VariablesTab } from './tabs/VariablesTab';
import { HistoryTab } from './tabs/HistoryTab';
import { LogsTab } from './tabs/LogsTab';

interface InstanceDetailDialogProps {
  instance: ProcessInstance | null;
  onClose: () => void;
  onDeleteRequest: (id: string) => void;
  defMap: Map<string, DefinitionInfo>;
}

export function InstanceDetailDialog({
  instance,
  onClose,
  onDeleteRequest,
  defMap,
}: InstanceDetailDialogProps) {
  const { toast } = useToast();

  const [selected, setSelected] = useState<ProcessInstance | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [definitionXml, setDefinitionXml] = useState<string | null>(null);

  const [pendingTasks, setPendingTasks] = useState<PendingUserTask[]>([]);
  const [pendingServiceTasks, setPendingServiceTasks] = useState<PendingServiceTask[]>([]);
  const [liveTokens, setLiveTokens] = useState<ProcessInstance['tokens']>(undefined);

  const [variables, setVariables] = useState<VariableRow[]>([]);
  const [deletedKeys, setDeletedKeys] = useState<Set<string>>(new Set());
  const [historyRefreshTrigger, setHistoryRefreshTrigger] = useState(0);

  const [activeTab, setActiveTab] = useState('overview');
  const [editMode, setEditMode] = useState(false);

  const [tokenMoveOpen, setTokenMoveOpen] = useState(false);
  const [migrationOpen, setMigrationOpen] = useState(false);

  const isSuspended = !!(selected && typeof selected.state === 'object' && 'Suspended' in selected.state);
  const isCompleted = !!(selected && (
    selected.state === 'Completed' ||
    (typeof selected.state === 'object' && 'CompletedWithError' in selected.state)
  ));

  const loadPendingTasks = useCallback(async (details: ProcessInstance) => {
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
  }, []);

  const loadInstanceDetails = useCallback(async (inst: ProcessInstance) => {
    setDetailLoading(true);
    setDefinitionXml(null);
    try {
      const details = await getInstanceDetails(inst.id);
      setSelected(details);
      setLiveTokens(details.tokens);
      setVariables(parseVariables(details.variables));
      setHistoryRefreshTrigger(n => n + 1);

      try {
        const xml = await getDefinitionXml(details.definition_key);
        setDefinitionXml(xml);
      } catch { /* XML optional */ }

      await loadPendingTasks(details);
    } catch {
      setSelected(inst);
      setVariables(parseVariables(inst.variables));
      setPendingTasks([]);
      setPendingServiceTasks([]);
    } finally {
      setDetailLoading(false);
    }
  }, [loadPendingTasks]);

  useEffect(() => {
    if (!instance) {
      setSelected(null);
      setDefinitionXml(null);
      setEditMode(false);
      setActiveTab('overview');
      return;
    }
    if (!selected || instance.id !== selected.id) {
      loadInstanceDetails(instance);
    }
  }, [instance]);

  const liveRefresh = useCallback(async () => {
    if (!selected) return;
    try {
      const details = await getInstanceDetails(selected.id);
      setVariables(parseVariables(details.variables));
      setLiveTokens(details.tokens);
    } catch { /* silent */ }
  }, [selected?.id]);

  useEffect(() => {
    if (!selected) return;
    const id = setInterval(liveRefresh, 30000);
    return () => clearInterval(id);
  }, [selected?.id, liveRefresh]);

  useEngineEvents(liveRefresh, ['instance_changed', 'task_changed'], !!selected);

  const refreshDetails = useCallback(async () => {
    if (!selected) return;
    try {
      const details = await getInstanceDetails(selected.id);
      setSelected(details);
      setLiveTokens(details.tokens);
      setVariables(parseVariables(details.variables));
      setHistoryRefreshTrigger(n => n + 1);
      await loadPendingTasks(details);
    } catch { /* silent */ }
  }, [selected?.id, loadPendingTasks]);

  const handleSaveVariables = async () => {
    if (!selected) return;
    const varsToSave = serializeVariables(variables, deletedKeys);
    if (varsToSave === null) {
      toast({ variant: 'destructive', description: 'Ungültiges Variablenformat (JSON oder Zahlen prüfen)' });
      return;
    }
    try {
      await updateInstanceVariables(selected.id, varsToSave);
      toast({ description: 'Variablen gespeichert.' });
      const updated = await getInstanceDetails(selected.id);
      setSelected(updated);
      setVariables(parseVariables(updated.variables));
      setDeletedKeys(new Set());
      setHistoryRefreshTrigger(n => n + 1);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Fehler beim Speichern: ' + e });
    }
  };

  const handleSuspendResume = async () => {
    if (!selected) return;
    try {
      if (isSuspended) {
        await resumeInstance(selected.id);
        toast({ description: 'Instanz fortgesetzt.' });
      } else {
        await suspendInstance(selected.id);
        toast({ description: 'Instanz suspendiert.' });
      }
      refreshDetails();
    } catch (e: any) {
      toast({ variant: 'destructive', description: String(e) });
    }
  };

  const varCount = Object.keys(selected?.variables || {}).filter(k => !k.startsWith('_')).length;
  const logCount = selected?.audit_log?.length || 0;

  return (
    <Dialog open={!!instance} onOpenChange={open => !open && onClose()}>
      <DialogContent className="instance-detail max-w-[72vw] w-full h-[90vh] flex flex-col p-0 overflow-hidden bg-background">
        <DialogTitle className="sr-only">Instanz-Details</DialogTitle>
        <DialogDescription className="sr-only">Details und Variablen der Prozessinstanz</DialogDescription>

        {selected && (
          <InstanceDetailHeader
            instance={selected}
            defMap={defMap}
            editMode={editMode}
            onToggleEditMode={() => setEditMode(false)}
            onRefresh={refreshDetails}
            onDelete={() => onDeleteRequest(selected.id)}
            onClose={onClose}
            onTokenMove={() => setTokenMoveOpen(true)}
            onMigrate={() => setMigrationOpen(true)}
            onSuspendResume={handleSuspendResume}
            isSuspended={isSuspended}
            isCompleted={isCompleted}
          />
        )}

        {detailLoading || !selected ? (
          <div className="flex-1 flex items-center justify-center text-muted-foreground text-sm">
            Lade Instanz-Details…
          </div>
        ) : (
          <Tabs
            value={activeTab}
            onValueChange={val => {
              setActiveTab(val);
              // Beim Wechsel zu Variables im Edit-Mode: Edit-Mode anbieten
              if (val !== 'variables') setEditMode(false);
            }}
            className="flex flex-col flex-1 min-h-0"
          >
            <div className="px-6 border-b bg-background shrink-0 flex items-center justify-between">
              <TabsList className="h-12 bg-transparent gap-0 rounded-none p-0">
                <TabsTrigger
                  value="overview"
                  className="h-12 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent px-4 font-medium"
                >
                  Übersicht
                </TabsTrigger>
                <TabsTrigger
                  value="variables"
                  className="h-12 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent px-4 font-medium"
                >
                  Variablen {varCount > 0 && <span className="ml-1.5 text-xs text-muted-foreground">({varCount})</span>}
                </TabsTrigger>
                <TabsTrigger
                  value="history"
                  className="h-12 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent px-4 font-medium"
                >
                  History
                </TabsTrigger>
                <TabsTrigger
                  value="logs"
                  className="h-12 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent px-4 font-medium"
                >
                  Logs {logCount > 0 && <span className="ml-1.5 text-xs text-muted-foreground">({logCount})</span>}
                </TabsTrigger>
              </TabsList>

              {/* Edit-Mode-Toggle — nur im Variables-Tab sichtbar */}
              {activeTab === 'variables' && !isCompleted && (
                <Button
                  variant={editMode ? 'default' : 'outline'}
                  size="sm"
                  className={editMode
                    ? 'gap-1.5 bg-amber-500 hover:bg-amber-600 text-white border-amber-500'
                    : 'gap-1.5'
                  }
                  onClick={() => setEditMode(e => !e)}
                >
                  <Unlock className="h-4 w-4" />
                  {editMode ? 'Edit-Modus aktiv' : 'Edit-Modus'}
                </Button>
              )}
            </div>

            <div className="flex-1 overflow-y-auto min-h-0">
              <TabsContent value="overview" className="mt-0 h-full">
                <OverviewTab
                  instance={selected}
                  liveTokens={liveTokens}
                  definitionXml={definitionXml}
                  defMap={defMap}
                  pendingTasks={pendingTasks}
                  pendingServiceTasks={pendingServiceTasks}
                  onNodeClick={() => {}}
                />
              </TabsContent>

              <TabsContent value="variables" className="mt-0">
                <VariablesTab
                  instance={selected}
                  variables={variables}
                  onChange={setVariables}
                  deletedKeys={deletedKeys}
                  onDeletedKeysChange={setDeletedKeys}
                  editMode={editMode}
                  onSave={handleSaveVariables}
                  onRefresh={() => loadInstanceDetails(selected)}
                />
              </TabsContent>

              <TabsContent value="history" className="mt-0">
                <HistoryTab instanceId={selected.id} refreshTrigger={historyRefreshTrigger} />
              </TabsContent>

              <TabsContent value="logs" className="mt-0">
                <LogsTab
                  instanceId={selected.id}
                  auditLog={selected.audit_log}
                  refreshTrigger={historyRefreshTrigger}
                />
              </TabsContent>
            </div>
          </Tabs>
        )}

        <TokenMoveDialog
          instance={selected}
          xml={definitionXml}
          open={tokenMoveOpen}
          onClose={() => setTokenMoveOpen(false)}
          onMoved={() => { setTokenMoveOpen(false); refreshDetails(); }}
        />
        <MigrationDialog
          instance={selected}
          definitions={[...defMap.values()]}
          open={migrationOpen}
          onClose={() => setMigrationOpen(false)}
          onMigrated={() => { setMigrationOpen(false); refreshDetails(); }}
        />
      </DialogContent>
    </Dialog>
  );
}

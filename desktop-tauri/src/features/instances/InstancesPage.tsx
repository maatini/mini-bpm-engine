import { useState } from 'react';
import { RefreshCw, Activity, CheckCircle, Clock, Trash, FileCode2, Network, ScrollText, Layers } from 'lucide-react';
import { type ProcessInstance, type DefinitionInfo } from '../../shared/types/engine';
import { listInstances, listDefinitions, deleteInstance } from '../../shared/lib/tauri';
import { usePolling } from '../../shared/hooks/use-polling';
import { PageHeader } from '../../shared/components/PageHeader';
import { EmptyState } from '../../shared/components/EmptyState';
import { groupInstances, stateBadgeClass, stateLabel } from './InstanceStateUtils';
import { InstanceDetailDialog } from './InstanceDetailDialog';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { ScrollArea } from '@/components/ui/scroll-area';
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog';
import { cn } from '@/lib/utils';

export function InstancesPage({ selectedInstanceId, onClearSelection }: { selectedInstanceId?: string | null, onClearSelection?: () => void }) {
  const { toast } = useToast();
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ProcessInstance | null>(null);
  
  const [instanceToDelete, setInstanceToDelete] = useState<string | null>(null);

  const fetchData = async () => {
    setLoading(true);
    setError(null);
    try {
      const [instList, defList] = await Promise.all([listInstances(), listDefinitions()]);
      instList.sort((a: ProcessInstance, b: ProcessInstance) => a.id.localeCompare(b.id));
      setInstances(instList);
      setDefinitions(defList);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  usePolling(fetchData, 3000, !selected);

  // Initial selection if driven from outside (e.g. Incidents list)
  if (selectedInstanceId && instances.length > 0 && (!selected || selected.id !== selectedInstanceId)) {
    const inst = instances.find(i => i.id === selectedInstanceId);
    if (inst) {
      setSelected(inst);
      if (onClearSelection) onClearSelection();
    }
  }

  const handleDeleteRequest = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    setInstanceToDelete(id);
  };

  const confirmDelete = async () => {
    if (!instanceToDelete) return;
    try {
      await deleteInstance(instanceToDelete);
      if (selected?.id === instanceToDelete) {
        setSelected(null);
      }
      fetchData();
    } catch (err) {
      toast({ variant: 'destructive', description: "Failed to delete instance: " + err });
    } finally {
      setInstanceToDelete(null);
    }
  };

  const { groups, unknownGroup, defMap } = groupInstances(instances, definitions);

  return (
    <div className="flex flex-col h-full bg-background relative overflow-hidden">
      <PageHeader 
        title="Instances" 
        actions={
          <>
            <span className="text-xs text-muted-foreground bg-muted px-2 py-1 rounded-md">Auto-refreshing</span>
            <Button onClick={fetchData} variant="outline" size="sm" className="gap-2">
              <RefreshCw className="h-4 w-4" /> Refresh
            </Button>
          </>
        }
      />

      <ScrollArea className="flex-1 w-full h-[calc(100vh-73px)]">
        <div className="p-6 space-y-6">
          {loading && instances.length === 0 && (
            <div className="space-y-4">
              {[1,2,3].map(i => (
                <Card key={i} className="p-4">
                  <div className="flex items-center gap-4">
                    <Skeleton className="h-6 w-[120px] rounded-full" />
                    <Skeleton className="h-5 w-[200px]" />
                    <Skeleton className="h-5 w-[80px] ml-auto" />
                  </div>
                </Card>
              ))}
            </div>
          )}
          
          {error && <div className="text-destructive font-medium bg-destructive/10 p-4 rounded border border-destructive/20 text-sm">Error: {error}</div>}
          
          {!loading && !error && instances.length === 0 && (
            <EmptyState 
              icon={Layers}
              title="No Instances Yet"
              description="Deploy a BPMN process and start your first instance from the Modeler."
            />
          )}

          {[...groups.entries()].map(([bpmnId, groupInstances]) => {
            const activeCount = groupInstances.filter(i => i.state !== 'Completed').length;
            
            return (
              <Card key={bpmnId} className="process-group-card overflow-hidden">
                <CardHeader className="bg-muted/40 py-4 flex flex-row items-center justify-between border-b">
                  <div className="flex items-center gap-2">
                    <FileCode2 className="h-5 w-5 text-primary" />
                    <CardTitle className="text-xl">{bpmnId}</CardTitle>
                  </div>
                  <div className="flex gap-2">
                    <Badge variant="secondary">{groupInstances.length} total</Badge>
                    {activeCount > 0 && <Badge variant="default" className="bg-yellow-500/20 text-yellow-700 hover:bg-yellow-500/30 border-yellow-500/50 dark:text-yellow-400">{activeCount} active</Badge>}
                  </div>
                </CardHeader>
                <CardContent className="p-0">
                  <div className="divide-y text-sm">
                    {groupInstances.map(inst => {
                      const def = defMap.get(inst.definition_key);
                      const varCount = Object.keys(inst.variables || {}).length;
                      const logCount = inst.audit_log?.length || 0;
                      
                      return (
                        <div
                          key={inst.id}
                          className="instance-list-item flex items-center justify-between p-4 hover:bg-accent/50 cursor-pointer transition-colors"
                          onClick={() => setSelected(inst)}
                        >
                          <div className="flex gap-6 items-center flex-1">
                            <div className="w-[140px]">
                              <Badge className={cn(
                                "flex items-center justify-center gap-1.5 w-full",
                                stateBadgeClass(inst.state)
                              )}>
                                {inst.state === 'Running' && <Activity className="h-3 w-3" />}
                                {inst.state === 'Completed' && <CheckCircle className="h-3 w-3" />}
                                {typeof inst.state === 'object' && <Clock className="h-3 w-3" />}
                                {stateLabel(inst.state)}
                              </Badge>
                            </div>
                            <div className="flex flex-col gap-1">
                              <span className="font-semibold">
                                {inst.business_key || inst.id.substring(0, 8)} 
                                <span className="font-normal text-muted-foreground ml-2">(#{inst.id.substring(0, 8)})</span>
                              </span>
                              <span className="text-xs text-muted-foreground flex items-center gap-1.5">
                                <Network className="h-3 w-3" /> 
                                {inst.state === 'Completed' 
                                  ? <span className="italic">Process ended</span> 
                                  : inst.current_node}
                              </span>
                            </div>
                          </div>

                          <div className="flex items-center gap-3">
                            {def && (
                              <Badge variant="outline" className={cn("font-mono text-xs", def.is_latest ? "bg-blue-500/10 text-blue-700 border-blue-500/30 dark:text-blue-400" : "")} title={`ByKey: ${def.key}`}>
                                v{def.version}
                              </Badge>
                            )}
                            <Badge variant="secondary" className="flex items-center gap-1"><ScrollText className="h-3 w-3"/>{varCount}</Badge>
                            <Badge variant="secondary" className="flex items-center gap-1"><Activity className="h-3 w-3"/>{logCount}</Badge>
                            
                            <Button
                              variant="ghost"
                              size="icon"
                              className="text-destructive hover:bg-destructive/10 hover:text-destructive h-8 w-8 ml-2"
                              onClick={(e) => handleDeleteRequest(e, inst.id)}
                            >
                              <Trash className="h-4 w-4" />
                            </Button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </CardContent>
              </Card>
            );
          })}

          {unknownGroup.length > 0 && (
            <Card className="opacity-80">
               <CardHeader className="bg-muted py-3">
                  <CardTitle className="text-lg text-muted-foreground">Unknown Definitions</CardTitle>
                </CardHeader>
                <CardContent className="p-0">
                  <div className="divide-y text-sm">
                    {unknownGroup.map(inst => (
                       <div key={inst.id} className="instance-list-item flex items-center gap-6 p-4 hover:bg-accent/50 cursor-pointer" onClick={() => setSelected(inst)}>
                         <Badge className={stateBadgeClass(inst.state)}>{stateLabel(inst.state)}</Badge>
                         <span className="font-medium">{inst.business_key || inst.id.substring(0, 8)}</span>
                         <span className="text-muted-foreground">{inst.definition_key.substring(0, 8)}…</span>
                       </div>
                    ))}
                  </div>
                </CardContent>
            </Card>
          )}
        </div>
      </ScrollArea>

      <InstanceDetailDialog 
        instance={selected} 
        onClose={() => setSelected(null)} 
        onDeleteRequest={(id: string) => setInstanceToDelete(id)}
        defMap={defMap}
      />

      <AlertDialog open={!!instanceToDelete} onOpenChange={open => !open && setInstanceToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Instance</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete this process instance? This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={confirmDelete} className="bg-destructive hover:bg-destructive/90 text-destructive-foreground">Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

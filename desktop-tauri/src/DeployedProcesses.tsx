import { useState, useEffect, useCallback } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { listDefinitions, getDefinitionXml, listInstances, deleteDefinition, deleteAllDefinitions, type DefinitionInfo, type ProcessInstance } from './lib/tauri';
import { RefreshCw, Eye, Download, Activity, Clock, Trash, FileCode2, Network, Key, Boxes, Database } from 'lucide-react';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from '@/components/ui/accordion';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog';
import { cn } from '@/lib/utils';

function groupByProcess(defs: DefinitionInfo[]): Map<string, DefinitionInfo[]> {
  const map = new Map<string, DefinitionInfo[]>();
  for (const d of defs) {
    const existing = map.get(d.bpmn_id) || [];
    existing.push(d);
    map.set(d.bpmn_id, existing);
  }
  for (const [, versions] of map) {
    versions.sort((a, b) => b.version - a.version);
  }
  return map;
}

export function DeployedProcesses({ onView, onViewInstance }: { onView: (xml: string) => void, onViewInstance?: (id: string) => void }) {
  const { toast } = useToast();
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [viewingId, setViewingId] = useState<string | null>(null);
  const [deleteRequest, setDeleteRequest] = useState<{defId: string, bpmnId?: string, isAll: boolean, cascade: boolean, msg: string} | null>(null);
  
  const fetchDefinitions = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [list, instList] = await Promise.all([listDefinitions(), listInstances()]);
      setDefinitions(list);
      setInstances(instList);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDefinitions();
  }, [fetchDefinitions]);

  const handleDownload = async (defId: string) => {
    setDownloading(defId);
    try {
      const xml = await getDefinitionXml(defId);
      const filePath = await save({
        defaultPath: `definition-${defId.substring(0, 8)}.bpmn`,
        filters: [{ name: 'BPMN', extensions: ['bpmn', 'xml'] }],
      });
      if (filePath) {
        await writeTextFile(filePath, xml);
      }
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Download failed: ' + e });
    } finally {
      setDownloading(null);
    }
  };

  const handleView = async (defId: string) => {
    setViewingId(defId);
    try {
      const xml = await getDefinitionXml(defId);
      onView(xml);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Failed to load definition: ' + e });
    } finally {
      setViewingId(null);
    }
  };

  const handleDeleteCheck = (defId: string) => {
    const relatedInstances = instances.filter(i => i.definition_key === defId);
    let cascade = false;
    let msg = "Are you sure you want to delete this process definition version?";

    if (relatedInstances.length > 0) {
      msg = `This version has ${relatedInstances.length} associated instance(s). Deleting it will also permanently delete all associated instances.\n\nAre you sure?`;
      cascade = true;
    }
    setDeleteRequest({ defId, isAll: false, cascade, msg });
  };

  const handleDeleteAllCheck = (bpmnId: string, versions: DefinitionInfo[]) => {
    const versionKeys = versions.map(v => v.key);
    const relatedInstances = instances.filter(i => versionKeys.includes(i.definition_key));
    let cascade = false;
    
    const versionListInfo = versions.length > 1 
      ? `\n\nVersions to be deleted: ${versions.length}` 
      : '';

    let msg = `Are you sure you want to delete ALL versions of process "${bpmnId}"?${versionListInfo}`;

    if (relatedInstances.length > 0) {
      msg = `Process "${bpmnId}" has ${relatedInstances.length} associated instance(s) across all versions.${versionListInfo}\n\nDeleting the entire deployment will also permanently delete all associated instances.\n\nAre you absolutely sure?`;
      cascade = true;
    }
    setDeleteRequest({ defId: '', bpmnId, isAll: true, cascade, msg });
  };

  const confirmDelete = async () => {
    if (!deleteRequest) return;
    try {
      if (deleteRequest.isAll && deleteRequest.bpmnId) {
        await deleteAllDefinitions(deleteRequest.bpmnId, deleteRequest.cascade);
      } else {
        await deleteDefinition(deleteRequest.defId, deleteRequest.cascade);
      }
      fetchDefinitions();
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Delete failed: ' + e });
    } finally {
      setDeleteRequest(null);
    }
  };

  const grouped = groupByProcess(definitions);

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-6 py-4 border-b bg-background">
        <h2 className="text-2xl font-bold tracking-tight">Deployed Processes</h2>
        <Button onClick={fetchDefinitions} variant="outline" size="sm" className="gap-2">
          <RefreshCw className="h-4 w-4" /> Refresh
        </Button>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-6">
          {loading && (
            <div className="space-y-4">
              {[1,2,3].map(i => (
                <Card key={i} className="p-4">
                  <div className="flex items-center gap-4">
                    <Skeleton className="h-10 w-10 rounded" />
                    <div className="space-y-2">
                      <Skeleton className="h-5 w-[200px]" />
                      <Skeleton className="h-4 w-[150px]" />
                    </div>
                  </div>
                </Card>
              ))}
            </div>
          )}
          {error && <div className="text-destructive font-medium">Error: {error}</div>}
          {!loading && !error && grouped.size === 0 && (
            <div className="flex flex-col items-center justify-center py-20 text-center">
              <Database className="h-16 w-16 text-muted-foreground/30 mb-4" />
              <h3 className="text-lg font-semibold text-muted-foreground">No Deployed Processes</h3>
              <p className="text-sm text-muted-foreground/70 mt-1 max-w-sm">
                Deploy a BPMN process from the Modeler to see it listed here.
              </p>
            </div>
          )}

          {[...grouped.entries()].map(([bpmnId, versions]) => {
            const latest = versions[0];
            const olderVersions = versions.slice(1);
            const instancesForProcess = instances.filter(i => versions.some(v => v.key === i.definition_key) && i.state !== 'Completed');

            return (
              <Card key={bpmnId} className="overflow-hidden">
                <CardHeader className="bg-muted/40 py-4 flex flex-row items-center justify-between border-b">
                  <div className="flex items-center gap-2">
                    <FileCode2 className="h-5 w-5 text-primary" />
                    <CardTitle className="text-xl">{bpmnId}</CardTitle>
                  </div>
                  <div className="flex items-center gap-2">
                    <Badge variant="secondary">{versions.length} deployed version{versions.length > 1 ? 's' : ''}</Badge>
                    {instancesForProcess.length > 0 && (
                      <Badge variant="default" className="bg-yellow-500/20 text-yellow-700 hover:bg-yellow-500/30 border-yellow-500/50 dark:text-yellow-400">
                        {instancesForProcess.length} active instance{instancesForProcess.length > 1 ? 's' : ''}
                      </Badge>
                    )}
                    <div className="ml-2 pl-2 border-l border-muted-foreground/20">
                      <Button variant="outline" onClick={() => handleDeleteAllCheck(bpmnId, versions)} size="sm" className="text-destructive hover:bg-destructive/10 hover:text-destructive h-7 px-2 gap-1.5" title="Delete entire deployment (all versions)">
                        <Trash className="h-3.5 w-3.5" /> <span className="text-xs">Delete All</span>
                      </Button>
                    </div>
                  </div>
                </CardHeader>

                <CardContent className="p-6 space-y-6">
                  {/* LATEST VERSION BOX */}
                  <div className="bg-muted/30 border rounded-lg p-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
                    <div className="flex flex-col gap-3">
                      <div className="flex items-center gap-3">
                        <Badge className="bg-blue-500/10 text-blue-700 border-blue-500/30 dark:text-blue-400 hover:bg-blue-500/20 font-mono text-sm">
                          v{latest.version} (Latest)
                        </Badge>
                      </div>
                      <div className="flex items-center gap-4 text-sm text-muted-foreground">
                        <span className="flex items-center gap-1.5"><Network className="h-4 w-4"/> {latest.node_count} process nodes</span>
                        <span className="flex items-center gap-1.5"><Key className="h-4 w-4"/> Key: {latest.key.substring(0, 16)}…</span>
                      </div>
                    </div>
                    
                    <div className="flex flex-wrap gap-2">
                      <Button onClick={() => handleView(latest.key)} disabled={viewingId === latest.key} size="sm" className="gap-2">
                        <Eye className="h-4 w-4" /> {viewingId === latest.key ? 'Loading...' : 'View BPMN'}
                      </Button>
                      <Button variant="outline" onClick={() => handleDownload(latest.key)} disabled={downloading === latest.key} size="sm" className="gap-2">
                        <Download className="h-4 w-4" /> Download
                      </Button>
                      <Button variant="outline" onClick={() => handleDeleteCheck(latest.key)} size="icon" className="text-destructive hover:bg-destructive/10 hover:text-destructive h-9 w-9" title="Delete latest version">
                        <Trash className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>

                  <Accordion type="multiple" className="w-full">
                    {/* OLDER VERSIONS */}
                    {olderVersions.length > 0 && (
                      <AccordionItem value="older-versions" className="border rounded-md px-4 bg-muted/10 mb-4">
                        <AccordionTrigger className="hover:no-underline text-sm font-semibold [&[data-state=open]]:text-primary">
                          <span className="flex items-center gap-2">
                            <Boxes className="h-4 w-4 text-muted-foreground" /> Older Versions ({olderVersions.length})
                          </span>
                        </AccordionTrigger>
                        <AccordionContent className="pt-2 pb-4">
                          <div className="divide-y border-t mt-2">
                            {olderVersions.map(ver => {
                              const verInstances = instances.filter(i => i.definition_key === ver.key && i.state !== 'Completed');
                              return (
                                <div key={ver.key} className="flex items-center justify-between py-3">
                                  <div className="flex items-center gap-4 text-sm">
                                    <Badge variant="secondary" className="font-mono">v{ver.version}</Badge>
                                    <span className="flex items-center gap-1.5 text-muted-foreground"><Network className="h-3.5 w-3.5"/> {ver.node_count} nodes</span>
                                    <span className="flex items-center gap-1.5 text-muted-foreground"><Key className="h-3.5 w-3.5"/> {ver.key.substring(0, 8)}…</span>
                                    {verInstances.length > 0 && (
                                      <Badge variant="outline" className="bg-amber-500/10 text-amber-600 border-amber-500/20">{verInstances.length} active</Badge>
                                    )}
                                  </div>
                                  <div className="flex gap-2">
                                    <Button variant="ghost" size="icon" onClick={() => handleView(ver.key)} className="h-8 w-8">
                                      <Eye className="h-4 w-4" />
                                    </Button>
                                    <Button variant="ghost" size="icon" onClick={() => handleDownload(ver.key)} className="h-8 w-8">
                                      <Download className="h-4 w-4" />
                                    </Button>
                                    <Button variant="ghost" size="icon" onClick={() => handleDeleteCheck(ver.key)} className="text-destructive hover:bg-destructive/10 hover:text-destructive h-8 w-8">
                                      <Trash className="h-4 w-4" />
                                    </Button>
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        </AccordionContent>
                      </AccordionItem>
                    )}

                    {/* RUNNING INSTANCES */}
                    {instancesForProcess.length > 0 && (
                      <AccordionItem value="active-instances" className="border rounded-md px-4 bg-muted/10">
                        <AccordionTrigger className="hover:no-underline text-sm font-semibold [&[data-state=open]]:text-primary">
                          <span className="flex items-center gap-2">
                            <Activity className="h-4 w-4 text-green-500" /> Active Instances ({instancesForProcess.length})
                          </span>
                        </AccordionTrigger>
                        <AccordionContent className="pt-2 pb-4">
                          <div className="divide-y border-t mt-2">
                            {instancesForProcess.map(inst => {
                              const instDef = versions.find(v => v.key === inst.definition_key);
                              return (
                                <div key={inst.id} className="flex items-center justify-between py-3 hover:bg-accent/50 px-2 rounded-md cursor-pointer transition-colors" onClick={() => onViewInstance?.(inst.id)}>
                                  <div className="flex items-center gap-3">
                                    {inst.state === 'Running' ? <Activity className="h-4 w-4 text-green-500" /> : <Clock className="h-4 w-4 text-amber-500" />}
                                    <span className="font-semibold text-foreground">{inst.business_key || inst.id.substring(0, 8)}</span>
                                  </div>
                                  <div className="flex gap-4 items-center">
                                    {instDef && (
                                      <Badge variant={instDef.is_latest ? 'default' : 'secondary'} className={cn("font-mono text-xs", instDef.is_latest ? "bg-blue-500/10 text-blue-700 border-blue-500/30 dark:text-blue-400" : "")}>
                                        v{instDef.version}
                                      </Badge>
                                    )}
                                    <Badge variant="outline" className="text-xs">Current: {inst.current_node}</Badge>
                                  </div>
                                </div>
                              );
                            })}
                          </div>
                        </AccordionContent>
                      </AccordionItem>
                    )}
                  </Accordion>
                </CardContent>
              </Card>
            );
          })}
        </div>
      </ScrollArea>
      
      <AlertDialog open={!!deleteRequest} onOpenChange={open => !open && setDeleteRequest(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Deletion</AlertDialogTitle>
            <AlertDialogDescription className="whitespace-pre-wrap">
              {deleteRequest?.msg}
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

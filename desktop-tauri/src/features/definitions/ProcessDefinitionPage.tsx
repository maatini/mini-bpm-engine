import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  listDefinitions,
  getDefinitionXml,
  listInstances,
  getPendingServiceTasks,
  type DefinitionInfo,
  type ProcessInstance,
  type PendingServiceTask,
} from '../../shared/lib/tauri';
import { usePolling } from '../../shared/hooks/use-polling';
import { useEngineEvents } from '../../shared/hooks/use-engine-events';
import { useToast } from '@/hooks/use-toast';
import { Activity, AlertTriangle, ArrowLeft, RefreshCw, Clock, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { PageHeader } from '../../shared/components/PageHeader';

// @ts-ignore
import NavigatedViewer from 'bpmn-js/lib/NavigatedViewer';
import 'bpmn-js/dist/assets/diagram-js.css';
import 'bpmn-js/dist/assets/bpmn-font/css/bpmn-embedded.css';

interface Props {
  definitionKey: string;
  onBack: () => void;
  onViewInstance: (id: string) => void;
}

/** Treat Running + any Waiting* variant as active (same rule used elsewhere). */
function isActive(inst: ProcessInstance): boolean {
  if (inst.state === 'Completed') return false;
  if (typeof inst.state === 'object' && ('CompletedWithError' in inst.state || 'ErrorEnd' in (inst.state as any))) return false;
  return true;
}

function formatDateTime(s: string | null | undefined): string {
  if (!s) return '-';
  return new Date(s).toLocaleString();
}

export function ProcessDefinitionPage({ definitionKey, onBack, onViewInstance }: Props) {
  const { toast } = useToast();
  const containerRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<any>(null);
  const overlaysRef = useRef<string[]>([]);

  const [definition, setDefinition] = useState<DefinitionInfo | null>(null);
  const [xml, setXml] = useState<string | null>(null);
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [incidents, setIncidents] = useState<PendingServiceTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // Initial load of definition metadata + XML (doesn't change during polling)
  useEffect(() => {
    (async () => {
      try {
        const [defs, loadedXml] = await Promise.all([
          listDefinitions(),
          getDefinitionXml(definitionKey),
        ]);
        const def = defs.find(d => d.key === definitionKey) ?? null;
        setDefinition(def);
        setXml(loadedXml);
      } catch (e: any) {
        toast({ variant: 'destructive', description: 'Failed to load definition: ' + e });
      }
    })();
  }, [definitionKey, toast]);

  // Poll live state (instances + incidents) — filtered to this definition
  const fetchLiveState = useCallback(async () => {
    try {
      const [allInstances, allIncidents] = await Promise.all([
        listInstances(),
        getPendingServiceTasks(),
      ]);
      setInstances(allInstances.filter(i => i.definition_key === definitionKey && isActive(i)));
      setIncidents(allIncidents.filter(t => t.definition_key === definitionKey && t.retries <= 0));
      setLoading(false);
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Failed to load live state: ' + e });
    }
  }, [definitionKey, toast]);

  usePolling(fetchLiveState, 30000);
  useEngineEvents(fetchLiveState);

  // Aggregate token counts per node across all active instances
  const tokenCountsByNode = useMemo(() => {
    const m = new Map<string, number>();
    for (const inst of instances) {
      if (!inst.current_node) continue;
      m.set(inst.current_node, (m.get(inst.current_node) ?? 0) + 1);
    }
    return m;
  }, [instances]);

  const incidentCountsByNode = useMemo(() => {
    const m = new Map<string, number>();
    for (const inc of incidents) {
      if (!inc.node_id) continue;
      m.set(inc.node_id, (m.get(inc.node_id) ?? 0) + 1);
    }
    return m;
  }, [incidents]);

  // Mount the viewer once
  useEffect(() => {
    if (!containerRef.current) return;
    const viewer = new NavigatedViewer({ container: containerRef.current });
    viewerRef.current = viewer;
    return () => {
      viewer.destroy();
      viewerRef.current = null;
    };
  }, []);

  // Import XML + wire click-to-filter
  useEffect(() => {
    if (!viewerRef.current || !xml) return;
    let disposed = false;
    (async () => {
      try {
        await viewerRef.current.importXML(xml);
        if (disposed) return;
        const canvas = viewerRef.current.get('canvas');
        canvas.zoom('fit-viewport', 'auto');

        const eventBus = viewerRef.current.get('eventBus');
        eventBus.on('element.click', (e: any) => {
          const id = e?.element?.id;
          if (!id) return;
          setSelectedNodeId(prev => (prev === id ? null : id));
        });
      } catch (err) {
        console.error('Failed to import BPMN XML', err);
      }
    })();
    return () => { disposed = true; };
  }, [xml]);

  // Update overlays whenever counts or selection change
  useEffect(() => {
    if (!viewerRef.current || !xml) return;
    const overlays = viewerRef.current.get('overlays');
    const canvas = viewerRef.current.get('canvas');
    const elementRegistry = viewerRef.current.get('elementRegistry');

    // Clear previous overlays + markers
    for (const id of overlaysRef.current) {
      try { overlays.remove(id); } catch { /* ignore */ }
    }
    overlaysRef.current = [];

    elementRegistry.getAll().forEach((el: any) => {
      if (el.id) {
        canvas.removeMarker(el.id, 'pd-selected-node');
        canvas.removeMarker(el.id, 'pd-has-tokens');
        canvas.removeMarker(el.id, 'pd-has-incidents');
      }
    });

    // Token badges (Camunda 7 Cockpit style: blue pill, top-right)
    for (const [nodeId, count] of tokenCountsByNode) {
      const el = elementRegistry.get(nodeId);
      if (!el) continue;
      canvas.addMarker(nodeId, 'pd-has-tokens');
      const html = `<div class="pd-token-badge" title="${count} running instance${count === 1 ? '' : 's'} here">${count}</div>`;
      const oid = overlays.add(nodeId, 'pd-token-count', {
        position: { top: -10, right: 14 },
        html,
      });
      overlaysRef.current.push(oid);
    }

    // Incident badges (red pill with "!", top-left)
    for (const [nodeId, count] of incidentCountsByNode) {
      const el = elementRegistry.get(nodeId);
      if (!el) continue;
      canvas.addMarker(nodeId, 'pd-has-incidents');
      const html = `<div class="pd-incident-badge" title="${count} open incident${count === 1 ? '' : 's'}">!${count > 1 ? ` ${count}` : ''}</div>`;
      const oid = overlays.add(nodeId, 'pd-incident-count', {
        position: { top: -10, left: -10 },
        html,
      });
      overlaysRef.current.push(oid);
    }

    // Selection highlight
    if (selectedNodeId && elementRegistry.get(selectedNodeId)) {
      canvas.addMarker(selectedNodeId, 'pd-selected-node');
    }
  }, [tokenCountsByNode, incidentCountsByNode, selectedNodeId, xml]);

  const filteredInstances = useMemo(() => {
    if (!selectedNodeId) return instances;
    return instances.filter(i => i.current_node === selectedNodeId);
  }, [instances, selectedNodeId]);

  const totalActive = instances.length;
  const totalIncidents = incidents.length;

  return (
    <div className="flex flex-col h-full bg-background">
      <style>{css}</style>

      <PageHeader
        title={definition ? `${definition.bpmn_id} (v${definition.version})` : 'Process Definition'}
        subtitle={definition ? `Key: ${definition.key.substring(0, 16)}…` : undefined}
        icon={<Activity className="h-6 w-6 text-primary" />}
        actions={
          <div className="flex items-center gap-2">
            <Badge variant="outline" className="gap-1.5">
              <Activity className="h-3.5 w-3.5 text-green-500" /> {totalActive} active
            </Badge>
            {totalIncidents > 0 && (
              <Badge variant="outline" className="gap-1.5 text-destructive border-destructive/40">
                <AlertTriangle className="h-3.5 w-3.5" /> {totalIncidents} incidents
              </Badge>
            )}
            <Button variant="outline" size="sm" className="gap-2" onClick={fetchLiveState}>
              <RefreshCw className="h-4 w-4" /> Refresh
            </Button>
            <Button variant="outline" size="sm" className="gap-2" onClick={onBack}>
              <ArrowLeft className="h-4 w-4" /> Back
            </Button>
          </div>
        }
      />

      <div className="flex-1 flex min-h-0">
        {/* Diagram */}
        <div className="flex-1 relative border-r min-w-0">
          {!xml && (
            <div className="absolute inset-0 p-6 space-y-3">
              <Skeleton className="h-8 w-[200px]" />
              <Skeleton className="h-full w-full" />
            </div>
          )}
          <div ref={containerRef} className="w-full h-full bg-muted/10" data-testid="pd-canvas" />
          {selectedNodeId && (
            <div className="absolute top-3 left-3 flex items-center gap-2 bg-background/95 backdrop-blur border rounded-md px-3 py-1.5 text-sm shadow-sm">
              <span className="font-mono">{selectedNodeId}</span>
              <span className="text-muted-foreground">·</span>
              <span className="text-muted-foreground">{filteredInstances.length} instance{filteredInstances.length === 1 ? '' : 's'}</span>
              <Button size="icon" variant="ghost" className="h-5 w-5" onClick={() => setSelectedNodeId(null)} title="Clear filter">
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          )}
        </div>

        {/* Instances panel */}
        <div className="w-[380px] flex-shrink-0 flex flex-col bg-muted/10">
          <div className="px-4 py-3 border-b">
            <div className="text-sm font-semibold">Running Instances</div>
            <div className="text-xs text-muted-foreground">
              {selectedNodeId
                ? `Filtered to node '${selectedNodeId}' · ${filteredInstances.length}`
                : `All active · ${filteredInstances.length}`}
            </div>
          </div>

          <ScrollArea className="flex-1">
            <div className="p-2">
              {loading && instances.length === 0 && (
                <div className="space-y-2 p-2">
                  {[1, 2, 3].map(i => <Skeleton key={i} className="h-14 w-full" />)}
                </div>
              )}

              {!loading && filteredInstances.length === 0 && (
                <div className="p-6 text-center text-sm text-muted-foreground">
                  {selectedNodeId ? 'No instances at this node.' : 'No active instances.'}
                </div>
              )}

              {filteredInstances.map(inst => {
                const hasIncident = incidents.some(i => i.instance_id === inst.id);
                return (
                  <button
                    key={inst.id}
                    onClick={() => onViewInstance(inst.id)}
                    className="pd-instance-row w-full text-left px-3 py-2 rounded-md hover:bg-accent/60 flex items-start justify-between gap-2 group"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        {hasIncident ? (
                          <AlertTriangle className="h-3.5 w-3.5 text-destructive flex-shrink-0" />
                        ) : (
                          <Activity className="h-3.5 w-3.5 text-green-500 flex-shrink-0" />
                        )}
                        <span className="font-medium text-sm truncate">
                          {inst.business_key || inst.id.substring(0, 8)}
                        </span>
                      </div>
                      <div className="mt-0.5 flex items-center gap-2 text-[11px] text-muted-foreground">
                        <span className="font-mono">{inst.current_node}</span>
                        <span>·</span>
                        <Clock className="h-3 w-3" />
                        <span>{formatDateTime(inst.started_at)}</span>
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </ScrollArea>
        </div>
      </div>
    </div>
  );
}

const css = `
  .pd-token-badge {
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-radius: 11px;
    background: #2563eb;
    color: #fff;
    font-size: 11px;
    font-weight: 600;
    line-height: 22px;
    text-align: center;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
    pointer-events: none;
    font-family: ui-sans-serif, system-ui, sans-serif;
  }
  .pd-incident-badge {
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-radius: 11px;
    background: #dc2626;
    color: #fff;
    font-size: 11px;
    font-weight: 700;
    line-height: 22px;
    text-align: center;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
    pointer-events: none;
    font-family: ui-sans-serif, system-ui, sans-serif;
  }
  .pd-has-tokens:not(.djs-connection) .djs-visual > :nth-child(1) {
    stroke: #2563eb !important;
    stroke-width: 2.5px !important;
  }
  .pd-has-incidents:not(.djs-connection) .djs-visual > :nth-child(1) {
    stroke: #dc2626 !important;
    stroke-width: 2.5px !important;
  }
  .pd-selected-node:not(.djs-connection) .djs-visual > :nth-child(1) {
    stroke: #f59e0b !important;
    stroke-width: 3.5px !important;
    fill: rgba(245, 158, 11, 0.15) !important;
  }
`;

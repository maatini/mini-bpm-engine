import { useState } from 'react';
import { AlertTriangle, RotateCcw, CheckCircle, ExternalLink } from 'lucide-react';
import { type PendingServiceTask } from '../../shared/types/engine';
import { retryIncident, resolveIncident } from '../../shared/lib/tauri';
import { VariableEditor, type VariableRow, parseVariables, serializeVariables } from '../../shared/components/VariableEditor';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

export function IncidentDetailDialog({
  incident,
  onClose,
  onResolved,
  onViewInstance,
}: {
  incident: PendingServiceTask | null;
  onClose: () => void;
  onResolved: () => void;
  onViewInstance?: (id: string) => void;
}) {
  const { toast } = useToast();
  const [retries, setRetries] = useState(3);
  const [variables, setVariables] = useState<VariableRow[]>([]);
  const [loading, setLoading] = useState(false);

  // Reset variables when incident changes
  const [lastIncidentId, setLastIncidentId] = useState<string | null>(null);
  if (incident && incident.id !== lastIncidentId) {
    setLastIncidentId(incident.id);
    setVariables(parseVariables(incident.variables_snapshot || {}));
    setRetries(3);
  }

  const handleRetry = async () => {
    if (!incident) return;
    setLoading(true);
    try {
      await retryIncident(incident.id, retries);
      toast({ description: `Incident retried with ${retries} retries.` });
      onResolved();
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Retry failed: ' + e });
    } finally {
      setLoading(false);
    }
  };

  const handleResolve = async () => {
    if (!incident) return;
    setLoading(true);
    try {
      const vars = serializeVariables(variables, new Set());
      await resolveIncident(incident.id, vars || {});
      toast({ description: 'Incident resolved. Process continues.' });
      onResolved();
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Resolve failed: ' + e });
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={!!incident} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-[650px] w-full max-h-[85vh] flex flex-col p-0 overflow-hidden bg-background">
        <DialogHeader className="px-6 py-4 border-b bg-destructive/5 shrink-0">
          <DialogTitle className="text-lg flex items-center gap-2 text-destructive">
            <AlertTriangle className="h-5 w-5" /> Incident: {incident?.node_id}
          </DialogTitle>
        </DialogHeader>

        {incident && (
          <div className="flex-1 p-6 overflow-y-auto min-h-0 space-y-5">
            {/* Meta info */}
            <div className="grid grid-cols-2 gap-3 text-sm">
              <div>
                <span className="text-muted-foreground">Topic</span>
                <div><Badge variant="outline" className="font-mono mt-1">{incident.topic}</Badge></div>
              </div>
              <div>
                <span className="text-muted-foreground">Instance</span>
                <div className="flex items-center gap-2 mt-1">
                  <code className="bg-muted px-2 py-0.5 rounded text-xs">{incident.instance_id.substring(0, 8)}…</code>
                  {onViewInstance && (
                    <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => { onClose(); onViewInstance(incident.instance_id); }}>
                      <ExternalLink className="h-3.5 w-3.5" />
                    </Button>
                  )}
                </div>
              </div>
              <div>
                <span className="text-muted-foreground">Created</span>
                <div className="mt-1 text-xs">{new Date(incident.created_at).toLocaleString()}</div>
              </div>
              <div>
                <span className="text-muted-foreground">Worker</span>
                <div className="mt-1 text-xs">{incident.worker_id || 'None'}</div>
              </div>
            </div>

            {/* Error details */}
            <div className="space-y-2">
              <h4 className="font-semibold text-sm text-destructive">Error</h4>
              <div className="bg-destructive/10 p-3 rounded-md border border-destructive/20 text-sm">
                {incident.error_message || 'No error message'}
              </div>
              {incident.error_details && (
                <pre className="bg-muted border text-foreground p-3 rounded-md text-xs overflow-x-auto whitespace-pre-wrap font-mono max-h-[200px] overflow-y-auto">
                  {incident.error_details}
                </pre>
              )}
            </div>

            {/* Action: Retry */}
            <div className="border rounded-lg p-4 space-y-3">
              <h4 className="font-semibold text-sm flex items-center gap-2">
                <RotateCcw className="h-4 w-4" /> Retry
              </h4>
              <p className="text-xs text-muted-foreground">
                Resets the retry counter and clears the error so a worker can pick up the task again.
              </p>
              <div className="flex items-center gap-3">
                <Label htmlFor="retries" className="text-sm whitespace-nowrap">Retries:</Label>
                <Input
                  id="retries"
                  type="number"
                  min={1}
                  max={10}
                  value={retries}
                  onChange={(e) => setRetries(Number(e.target.value))}
                  className="w-20"
                />
                <Button onClick={handleRetry} disabled={loading} className="gap-2">
                  <RotateCcw className="h-4 w-4" /> Retry
                </Button>
              </div>
            </div>

            {/* Action: Resolve */}
            <div className="border rounded-lg p-4 space-y-3">
              <h4 className="font-semibold text-sm flex items-center gap-2">
                <CheckCircle className="h-4 w-4" /> Resolve Manually
              </h4>
              <p className="text-xs text-muted-foreground">
                Completes the task manually and advances the process to the next node. Optionally modify variables before resolving.
              </p>
              <VariableEditor
                variables={variables}
                onChange={setVariables}
                readOnlyNames={false}
              />
              <div className="flex justify-end">
                <Button onClick={handleResolve} disabled={loading} variant="default" className="gap-2">
                  <CheckCircle className="h-4 w-4" /> Resolve
                </Button>
              </div>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

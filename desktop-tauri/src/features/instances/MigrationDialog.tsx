import { useState, useEffect } from 'react';
import { AlertTriangle, GitBranch, Loader2, Plus, Trash2 } from 'lucide-react';
import { type ProcessInstance, type DefinitionInfo } from '../../shared/types/engine';
import { migrateInstance } from '../../shared/lib/tauri';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

interface MigrationDialogProps {
  instance: ProcessInstance | null;
  definitions: DefinitionInfo[];
  open: boolean;
  onClose: () => void;
  onMigrated: () => void;
}

interface MappingRow {
  id: number;
  from: string;
  to: string;
}

let _rowId = 0;
function nextRowId() {
  return ++_rowId;
}

export function MigrationDialog({ instance, definitions, open, onClose, onMigrated }: MigrationDialogProps) {
  const { toast } = useToast();
  const [targetKey, setTargetKey] = useState<string>('');
  const [mappingRows, setMappingRows] = useState<MappingRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [showConfirm, setShowConfirm] = useState(false);

  // Definitions with the same bpmn_id (excluding current version)
  const currentDef = instance ? definitions.find(d => d.key === instance.definition_key) : undefined;
  const candidates = currentDef
    ? definitions.filter(d => d.bpmn_id === currentDef.bpmn_id && d.key !== instance!.definition_key)
    : [];
  const targetDef = definitions.find(d => d.key === targetKey);

  // Reset when dialog opens / instance changes
  useEffect(() => {
    if (open) {
      setTargetKey('');
      setMappingRows([]);
      setShowConfirm(false);
    }
  }, [open, instance?.id]);

  const addMappingRow = () => {
    setMappingRows(prev => [...prev, { id: nextRowId(), from: '', to: '' }]);
  };

  const removeMappingRow = (id: number) => {
    setMappingRows(prev => prev.filter(r => r.id !== id));
  };

  const updateRow = (id: number, field: 'from' | 'to', value: string) => {
    setMappingRows(prev => prev.map(r => r.id === id ? { ...r, [field]: value } : r));
  };

  const handleMigrate = async () => {
    if (!instance || !targetKey) return;
    setLoading(true);
    try {
      const nodeMapping: Record<string, string> = {};
      for (const row of mappingRows) {
        if (row.from.trim() && row.to.trim()) {
          nodeMapping[row.from.trim()] = row.to.trim();
        }
      }
      await migrateInstance(instance.id, targetKey, Object.keys(nodeMapping).length > 0 ? nodeMapping : undefined);
      toast({
        description: `Instanz erfolgreich zu ${targetDef?.bpmn_id} v${targetDef?.version} migriert.`,
      });
      onMigrated();
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Migration fehlgeschlagen: ' + e });
    } finally {
      setLoading(false);
      setShowConfirm(false);
    }
  };

  const hasInvalidRows = mappingRows.some(r =>
    (r.from.trim() && !r.to.trim()) || (!r.from.trim() && r.to.trim())
  );

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg w-full flex flex-col p-0 overflow-hidden bg-background">
        <DialogHeader className="px-6 py-4 border-b shrink-0">
          <DialogTitle className="text-lg flex items-center gap-2">
            <GitBranch className="h-5 w-5 text-violet-500" />
            Instanz migrieren
          </DialogTitle>
          <DialogDescription className="sr-only">Instanz zu einer anderen Prozessversion migrieren</DialogDescription>
        </DialogHeader>

        {instance && (
          <div className="flex-1 overflow-y-auto p-6 space-y-5">
            {/* Current version info */}
            <div className="flex items-center gap-3 text-sm">
              <span className="text-muted-foreground">Aktuelle Version:</span>
              {currentDef ? (
                <Badge variant="outline" className="font-mono">
                  {currentDef.bpmn_id} v{currentDef.version}
                </Badge>
              ) : (
                <span className="font-mono text-muted-foreground">{instance.definition_key.substring(0, 8)}…</span>
              )}
            </div>

            {/* Target definition picker */}
            <div className="space-y-2">
              <Label className="text-sm font-semibold">Ziel-Definition</Label>
              {candidates.length === 0 ? (
                <div className="text-sm text-muted-foreground bg-muted/40 rounded-md px-3 py-2">
                  Keine anderen Versionen dieses Prozesses vorhanden.
                </div>
              ) : (
                <Select value={targetKey} onValueChange={setTargetKey}>
                  <SelectTrigger>
                    <SelectValue placeholder="Version auswählen…" />
                  </SelectTrigger>
                  <SelectContent>
                    {candidates.map(d => (
                      <SelectItem key={d.key} value={d.key}>
                        {d.bpmn_id} — v{d.version}{d.is_latest ? ' (latest)' : ''}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            </div>

            {/* Node Mapping */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <Label className="text-sm font-semibold">Node Mapping</Label>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    Wenn sich BPMN-Node-IDs in der neuen Version geändert haben, hier alt → neu eintragen.
                    Tokens auf unbekannten Nodes ohne Mapping werden abgelehnt.
                  </p>
                </div>
                <Button variant="outline" size="sm" className="gap-1.5 shrink-0" onClick={addMappingRow}>
                  <Plus className="h-3.5 w-3.5" /> Eintrag
                </Button>
              </div>

              {mappingRows.length === 0 && (
                <p className="text-xs text-muted-foreground italic">
                  Kein Mapping nötig, wenn alle Node-IDs gleich geblieben sind.
                </p>
              )}

              <div className="space-y-2">
                {mappingRows.map(row => (
                  <div key={row.id} className="flex items-center gap-2">
                    <Input
                      placeholder="Alte Node-ID"
                      value={row.from}
                      onChange={e => updateRow(row.id, 'from', e.target.value)}
                      className="font-mono text-xs h-8"
                    />
                    <span className="text-muted-foreground text-sm shrink-0">→</span>
                    <Input
                      placeholder="Neue Node-ID"
                      value={row.to}
                      onChange={e => updateRow(row.id, 'to', e.target.value)}
                      className="font-mono text-xs h-8"
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 shrink-0 text-muted-foreground hover:text-destructive"
                      onClick={() => removeMappingRow(row.id)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                ))}
              </div>
            </div>

            {/* Confirmation warning */}
            {showConfirm && targetKey && (
              <div className="bg-destructive/10 border border-destructive/30 rounded-lg p-4 space-y-3">
                <div className="flex items-start gap-2 text-sm">
                  <AlertTriangle className="h-5 w-5 text-destructive shrink-0 mt-0.5" />
                  <div>
                    <p className="font-semibold text-destructive">Migration ist nicht rückgängig zu machen!</p>
                    <p className="text-muted-foreground mt-1">
                      Die Instanz wechselt zu <strong>{targetDef?.bpmn_id} v{targetDef?.version}</strong>.
                      Alle Tokens werden auf die neuen Node-IDs umgezeigt.
                    </p>
                  </div>
                </div>
                <div className="flex justify-end gap-2">
                  <Button variant="outline" size="sm" onClick={() => setShowConfirm(false)}>
                    Abbrechen
                  </Button>
                  <Button
                    variant="destructive"
                    size="sm"
                    className="gap-2"
                    disabled={loading}
                    onClick={handleMigrate}
                  >
                    {loading
                      ? <Loader2 className="h-4 w-4 animate-spin" />
                      : <GitBranch className="h-4 w-4" />
                    }
                    Jetzt migrieren
                  </Button>
                </div>
              </div>
            )}
          </div>
        )}

        <DialogFooter className="px-6 py-4 border-t shrink-0">
          <Button variant="outline" onClick={onClose}>Schließen</Button>
          {!showConfirm && (
            <Button
              disabled={!targetKey || hasInvalidRows || candidates.length === 0}
              className="gap-2"
              onClick={() => setShowConfirm(true)}
            >
              <GitBranch className="h-4 w-4" />
              Migrieren…
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

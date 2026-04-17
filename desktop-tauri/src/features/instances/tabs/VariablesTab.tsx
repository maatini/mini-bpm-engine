import { type ProcessInstance } from '../../../shared/types/engine';
import { VariableEditor, type VariableRow } from '../../../shared/components/VariableEditor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { cn } from '@/lib/utils';

interface VariablesTabProps {
  instance: ProcessInstance;
  variables: VariableRow[];
  onChange: (vars: VariableRow[]) => void;
  deletedKeys: Set<string>;
  onDeletedKeysChange: (keys: Set<string>) => void;
  editMode: boolean;
  onSave: () => void;
  onRefresh: () => void;
}

function varTypeBadge(val: unknown) {
  if (val === null) return <Badge variant="outline" className="text-xs">null</Badge>;
  if (Array.isArray(val)) return <Badge variant="secondary" className="text-xs">array</Badge>;
  const t = typeof val;
  const cls =
    t === 'number' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400' :
    t === 'boolean' ? 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400' :
    t === 'object' ? 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400' :
    'bg-muted text-muted-foreground';
  return <Badge variant="secondary" className={cn('text-xs border-0', cls)}>{t}</Badge>;
}

function formatValue(val: unknown): string {
  if (val === null) return 'null';
  if (typeof val === 'object') return JSON.stringify(val, null, 2);
  return String(val);
}

export function VariablesTab({
  instance,
  variables,
  onChange,
  deletedKeys,
  onDeletedKeysChange,
  editMode,
  onSave,
  onRefresh,
}: VariablesTabProps) {
  const rawVars = instance.variables;
  const entries = Object.entries(rawVars).filter(([k]) => !k.startsWith('_'));

  if (!editMode) {
    return (
      <div className="p-6">
        {entries.length === 0 ? (
          <p className="text-sm text-muted-foreground italic">Keine Variablen gesetzt</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[200px]">Name</TableHead>
                <TableHead className="w-[100px]">Typ</TableHead>
                <TableHead>Wert</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {entries.map(([key, val]) => (
                <TableRow key={key}>
                  <TableCell className="font-mono font-medium">{key}</TableCell>
                  <TableCell>{varTypeBadge(val)}</TableCell>
                  <TableCell>
                    {typeof val === 'object' && val !== null ? (
                      <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap max-w-[400px]">
                        {formatValue(val)}
                      </pre>
                    ) : (
                      <span className="font-mono text-sm">{formatValue(val)}</span>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>
    );
  }

  return (
    <div className="p-6 space-y-4">
      <VariableEditor
        variables={variables}
        onChange={onChange}
        readOnlyNames={true}
        deletedKeys={deletedKeys}
        onDeletedKeysChange={onDeletedKeysChange}
        instanceId={instance.id}
        onVariablesRefreshRequest={onRefresh}
      />
      <div className="pt-4 border-t flex justify-end gap-2">
        <Button variant="outline" onClick={onRefresh}>Abbrechen</Button>
        <Button onClick={onSave}>Variablen speichern</Button>
      </div>
    </div>
  );
}

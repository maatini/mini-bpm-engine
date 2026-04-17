import { Trash, Paperclip, Download } from 'lucide-react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { uploadInstanceFile, downloadInstanceFile, type FileReference } from '../lib/tauri';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { useState } from 'react';

// Shared type definitions for the variable editor
export type VarType = 'String' | 'Number' | 'Boolean' | 'Object' | 'Null' | 'File';

function formatFileSize(bytes: number) {
  if (bytes < 1024) return bytes + ' B';
  const kb = bytes / 1024;
  if (kb < 1024) return kb.toFixed(1) + ' KB';
  const mb = kb / 1024;
  return mb.toFixed(1) + ' MB';
}

/** Type guard: checks if a value is a persisted FileReference object. */
function isFileReference(val: unknown): val is FileReference {
  return val !== null && typeof val === 'object' && (val as any).type === 'file';
}

export interface VariableRow {
  name: string;
  type: VarType;
  value: unknown;
  isNew?: boolean;
  /** Local file path for files that are not yet uploaded (deferred upload). */
  pendingFilePath?: string;
}

/**
 * Parse a variables record (as returned from the engine) into VariableRow[].
 */
export function parseVariables(vars: Record<string, unknown>): VariableRow[] {
  return Object.entries(vars)
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([name, val]) => {
      let type: VarType = 'String';
      if (val === null) type = 'Null';
      else if (typeof val === 'boolean') type = 'Boolean';
      else if (typeof val === 'number') type = 'Number';
      else if (typeof val === 'object' && val !== null && (val as any).type === 'file') type = 'File';
      else if (typeof val === 'object') type = 'Object';

      return {
        name,
        type,
        value: type === 'Object' ? JSON.stringify(val, null, 2) : val,
      };
    });
}

/**
 * Serialize VariableRow[] back into a plain Record<string, unknown>.
 * Returns null if validation fails. Caller should handle the error notification.
 */
export function serializeVariables(
  variables: VariableRow[],
  deletedKeys?: Set<string>,
): Record<string, unknown> | null {
  const result: Record<string, unknown> = {};

  // Mark explicitly deleted keys as null
  if (deletedKeys) {
    for (const key of deletedKeys) {
      result[key] = null;
    }
  }

  for (const v of variables) {
    if (!v.name.trim()) continue; // skip unnamed variables
    // Skip pending file rows – they are uploaded separately after instance creation
    if (v.type === 'File' && v.pendingFilePath) continue;

    if (v.type === 'Object') {
      try {
        result[v.name] = JSON.parse(v.value as string);
      } catch {
        return null;
      }
    } else if (v.type === 'Number') {
      const raw = String(v.value ?? '').trim();
      if (raw === '') { result[v.name] = 0; continue; }
      const num = Number(raw);
      if (isNaN(num)) {
        return null;
      }
      result[v.name] = num;
    } else if (v.type === 'Boolean') {
      result[v.name] = Boolean(v.value);
    } else if (v.type === 'Null') {
      result[v.name] = null;
    } else if (v.type === 'File') {
      result[v.name] = v.value;
    } else {
      result[v.name] = v.value;
    }
  }

  return result;
}

interface VariableEditorProps {
  variables: VariableRow[];
  onChange: (variables: VariableRow[]) => void;
  /** If true, existing variable names are read-only (used in Instance detail view). */
  readOnlyNames?: boolean;
  /** Track deleted keys for backend synchronisation (optional). */
  deletedKeys?: Set<string>;
  onDeletedKeysChange?: (keys: Set<string>) => void;
  instanceId?: string;
  onVariablesRefreshRequest?: () => void;
  /** Allow attaching files as pending (deferred upload, e.g. in start dialog). */
  allowPendingFiles?: boolean;
}

/**
 * Reusable typed variable editor table with Name / Type / Value columns
 * and Add / Remove controls.
 */
export function VariableEditor({
  variables,
  onChange,
  readOnlyNames = false,
  deletedKeys,
  onDeletedKeysChange,
  instanceId,
  onVariablesRefreshRequest,
  allowPendingFiles = false,
}: VariableEditorProps) {
  const { toast } = useToast();
  const [varNamePromptOpen, setVarNamePromptOpen] = useState(false);
  const [pendingFilePaths, setPendingFilePaths] = useState<string | null>(null);
  const [newVarName, setNewVarName] = useState('');

  const handleChange = (index: number, field: keyof VariableRow, newValue: unknown) => {
    const updated = [...variables];
    const row = { ...updated[index] };

    if (field === 'type') {
      row.type = newValue as VarType;
      if (row.type === 'String') row.value = '';
      else if (row.type === 'Number') row.value = '0';
      else if (row.type === 'Boolean') row.value = false;
      else if (row.type === 'Null') row.value = null;
      else if (row.type === 'Object') row.value = '{}';
      else if (row.type === 'File') { row.value = null; row.pendingFilePath = undefined; }
    } else {
      (row as Record<string, unknown>)[field] = newValue;
    }

    updated[index] = row;
    onChange(updated);
  };

  const handleAdd = () => {
    onChange([...variables, { name: '', type: 'String', value: '', isNew: true }]);
  };

  const handleUploadFile = async () => {
    try {
      const filePaths = await open({
        multiple: false,
        title: 'Select File to Upload'
      });
      if (!filePaths || Array.isArray(filePaths)) return;

      setPendingFilePaths(filePaths as string);
      setNewVarName('');
      setVarNamePromptOpen(true);
    } catch (e: any) {
      toast({ variant: 'destructive', description: `File upload failed: ${e}` });
    }
  };

  const submitNewVariable = async () => {
    const varName = newVarName.trim();
    const filePaths = pendingFilePaths;
    
    if (!varName || !filePaths) return;
    setVarNamePromptOpen(false);
    setPendingFilePaths(null);
    
    try {
      if (instanceId) {
        // Immediate upload to existing instance
        await uploadInstanceFile(instanceId, varName, filePaths);
        if (onVariablesRefreshRequest) {
          onVariablesRefreshRequest();
        }
      } else {
        // Deferred upload: store file path locally as a pending row
        const fileName = filePaths.split('/').pop() || 'file';
        const pendingRow: VariableRow = {
          name: varName,
          type: 'File',
          value: { filename: fileName, pendingPath: filePaths },
          isNew: true,
          pendingFilePath: filePaths,
        };
        onChange([...variables, pendingRow]);
      }
    } catch (e: any) {
      toast({ variant: 'destructive', description: `File handling failed: ${e}` });
    }
  };

  /** Opens the native file picker and attaches the selected file to an existing File-type row. */
  const handlePickFileForRow = async (index: number) => {
    try {
      const filePaths = await open({
        multiple: false,
        title: 'Select File to Attach'
      });
      if (!filePaths || Array.isArray(filePaths)) return;

      const updated = [...variables];
      const row = { ...updated[index] };
      const fileName = (filePaths as string).split('/').pop() || 'file';

      if (instanceId) {
        // Immediate upload to an existing instance
        await uploadInstanceFile(instanceId, row.name.trim(), filePaths as string);
        if (onVariablesRefreshRequest) {
          onVariablesRefreshRequest();
        }
      } else {
        // Deferred upload: store as pending file for upload after instance creation
        row.value = { filename: fileName, pendingPath: filePaths };
        row.pendingFilePath = filePaths as string;
        updated[index] = row;
        onChange(updated);
      }
    } catch (e: any) {
      toast({ variant: 'destructive', description: `File selection failed: ${e}` });
    }
  };

  const handleDownloadFile = async (varName: string, filename: string) => {
    if (!instanceId) return;
    try {
      const savePath = await save({
        defaultPath: filename,
        title: 'Save File'
      });
      if (!savePath) return;

      await downloadInstanceFile(instanceId, varName, savePath);
    } catch (e: any) {
      toast({ variant: 'destructive', description: `Download failed: ${e}` });
    }
  };

  const handleRemove = (index: number) => {
    const updated = [...variables];
    const removed = updated.splice(index, 1)[0];

    // Track removed keys that existed on the backend
    if (!removed.isNew && removed.name.trim() && deletedKeys && onDeletedKeysChange) {
      const newDeleted = new Set(deletedKeys);
      newDeleted.add(removed.name);
      onDeletedKeysChange(newDeleted);
    }

    onChange(updated);
  };

  return (
    <div className="space-y-4">
      <div className="border rounded-md overflow-hidden bg-background">
        <Table className="variables-table">
          <TableHeader className="bg-muted/40 hover:bg-muted/40">
            <TableRow>
              <TableHead className="w-[30%]">Name</TableHead>
              <TableHead className="w-[20%]">Type</TableHead>
              <TableHead>Value</TableHead>
              <TableHead className="w-12 text-center"></TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {variables.map((v, idx) => (
              <TableRow key={idx}>
                <TableCell className="p-2">
                  <Input
                    type="text"
                    value={v.name}
                    onChange={(e: any) => handleChange(idx, 'name', e.target.value)}
                    placeholder="Variable name"
                    readOnly={readOnlyNames && !v.isNew}
                    className={`var-input ${readOnlyNames && !v.isNew ? "bg-muted/50" : ""}`}
                    autoCapitalize="off"
                    autoComplete="off"
                    spellCheck={false}
                  />
                </TableCell>
                <TableCell className="p-2">
                  <select
                    className="flex h-10 w-full items-center justify-between rounded-md border border-input bg-background text-foreground px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                    value={v.type}
                    onChange={(e) => handleChange(idx, 'type', e.target.value)}
                    disabled={v.type === 'File'}
                  >
                    <option value="String">String</option>
                    <option value="Number">Number</option>
                    <option value="Boolean">Boolean</option>
                    <option value="Object">Object</option>
                    <option value="File">File</option>
                    <option value="Null">Null</option>
                  </select>
                </TableCell>
                <TableCell className="p-2">
                  {v.type === 'String' && (
                    <Input
                      type="text"
                      value={v.value as string}
                      onChange={(e: any) => handleChange(idx, 'value', e.target.value)}
                      placeholder="String value"
                      autoCapitalize="off"
                      autoComplete="off"
                      spellCheck={false}
                    />
                  )}
                  {v.type === 'Number' && (
                    <Input
                      type="text"
                      inputMode="decimal"
                      value={v.value == null || (typeof v.value === 'number' && isNaN(v.value as number)) ? '' : String(v.value)}
                      onChange={(e: any) => handleChange(idx, 'value', e.target.value)}
                      placeholder="z.B. 42 oder 3.14"
                      autoComplete="off"
                      spellCheck={false}
                    />
                  )}
                  {v.type === 'Boolean' && (
                    <div className="flex h-10 items-center pl-2">
                      <input
                        type="checkbox"
                        checked={v.value as boolean}
                        onChange={(e) => handleChange(idx, 'value', e.target.checked)}
                        className="var-checkbox h-4 w-4 rounded border-input text-primary focus:ring-primary disabled:cursor-not-allowed disabled:opacity-50"
                      />
                    </div>
                  )}
                  {v.type === 'Object' && (
                    <Textarea
                      value={v.value as string}
                      onChange={(e: any) => handleChange(idx, 'value', e.target.value)}
                      rows={2}
                      spellCheck={false}
                      className="min-h-[2.5rem] font-mono text-xs"
                    />
                  )}
                  {v.type === 'Null' && (
                    <div className="flex h-10 items-center pl-2">
                      <span className="text-muted-foreground italic text-sm">null</span>
                    </div>
                  )}
                  {v.type === 'File' && v.pendingFilePath && (
                    <div className="flex h-10 items-center gap-2">
                      <Badge variant="outline" className="gap-1 font-mono bg-muted/50">
                        <Paperclip className="h-3 w-3" />
                        {(v.value as any)?.filename || v.pendingFilePath.split('/').pop()}
                      </Badge>
                      <span className="text-xs text-muted-foreground italic">(Pending upload...)</span>
                    </div>
                  )}
                  {v.type === 'File' && !v.pendingFilePath && !isFileReference(v.value) && (
                    <div className="flex h-10 items-center gap-2">
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => handlePickFileForRow(idx)}
                        className="gap-1.5 h-8 text-xs"
                      >
                        <Paperclip className="h-3.5 w-3.5" /> Choose File…
                      </Button>
                      <span className="text-muted-foreground italic text-xs">
                        No file selected
                      </span>
                    </div>
                  )}
                  {v.type === 'File' && !v.pendingFilePath && isFileReference(v.value) && (
                    <div className="flex h-10 items-center justify-between gap-2 max-w-full group">
                      <div
                        onClick={() => {
                          if (instanceId) {
                            handleDownloadFile(v.name, (v.value as FileReference).filename);
                          }
                        }}
                        className={`flex items-center gap-2 overflow-hidden ${instanceId ? 'cursor-pointer hover:underline text-primary' : 'text-foreground'}`}
                        title={instanceId ? "Download file" : ""}
                      >
                        <Paperclip className="h-4 w-4 shrink-0" />
                        <span className="font-medium truncate text-sm">{(v.value as FileReference)?.filename}</span>
                        <span className="text-muted-foreground text-xs shrink-0">
                          ({formatFileSize((v.value as FileReference)?.size_bytes || 0)})
                        </span>
                      </div>
                      
                      {instanceId && (
                        <Button 
                          variant="ghost" 
                          size="sm"
                          onClick={() => handleDownloadFile(v.name, (v.value as FileReference).filename)}
                          className="h-8 gap-1.5 text-blue-600 hover:text-blue-700 hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-blue-900/20 shrink-0"
                        >
                          <Download className="h-3.5 w-3.5" /> Download
                        </Button>
                      )}
                    </div>
                  )}
                </TableCell>
                <TableCell className="p-2 text-center">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-destructive hover:text-destructive hover:bg-destructive/10"
                    onClick={() => handleRemove(idx)}
                    title="Delete Variable"
                  >
                    <Trash className="h-4 w-4" />
                  </Button>
                </TableCell>
              </TableRow>
            ))}
            {variables.length === 0 && (
              <TableRow>
                <TableCell colSpan={4} className="text-center text-muted-foreground py-8">
                  No variables configured.
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      <div className="flex items-center gap-3">
        <Button
          variant="outline"
          onClick={handleAdd}
          className="gap-2"
        >
          + Add Variable
        </Button>
        {(instanceId || allowPendingFiles) && (
          <Button
            variant="outline"
            onClick={handleUploadFile}
            className="gap-2"
          >
            <Paperclip className="h-4 w-4" /> Attach File
          </Button>
        )}
      </div>
      
      <Dialog open={varNamePromptOpen} onOpenChange={setVarNamePromptOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Enter Variable Name</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <Input 
              autoFocus
              placeholder="e.g. uploaded_document"
              value={newVarName}
              onChange={e => setNewVarName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && submitNewVariable()}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setVarNamePromptOpen(false)}>Cancel</Button>
            <Button onClick={submitNewVariable} disabled={!newVarName.trim()}>Add File Variable</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

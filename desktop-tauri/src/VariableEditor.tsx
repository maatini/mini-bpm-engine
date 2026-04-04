import { Trash, Paperclip, Download } from 'lucide-react';
import { open, save } from '@tauri-apps/api/dialog';
import { uploadInstanceFile, downloadInstanceFile, type FileReference } from './lib/tauri';

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
 * Returns null and shows an alert if validation fails.
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
        alert(`Invalid JSON for variable '${v.name}'`);
        return null;
      }
    } else if (v.type === 'Number') {
      const num = Number(v.value);
      if (isNaN(num)) {
        alert(`Invalid number for variable '${v.name}'`);
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
  const handleChange = (index: number, field: keyof VariableRow, newValue: unknown) => {
    const updated = [...variables];
    const row = { ...updated[index] };

    if (field === 'type') {
      row.type = newValue as VarType;
      if (row.type === 'String') row.value = '';
      else if (row.type === 'Number') row.value = 0;
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

      const varName = prompt('Enter a variable name for this file:');
      if (!varName || !varName.trim()) return;

      if (instanceId) {
        // Immediate upload to existing instance
        await uploadInstanceFile(instanceId, varName.trim(), filePaths as string);
        if (onVariablesRefreshRequest) {
          onVariablesRefreshRequest();
        }
      } else {
        // Deferred upload: store file path locally as a pending row
        const fileName = (filePaths as string).split('/').pop() || 'file';
        const pendingRow: VariableRow = {
          name: varName.trim(),
          type: 'File',
          value: { filename: fileName, pendingPath: filePaths },
          isNew: true,
          pendingFilePath: filePaths as string,
        };
        onChange([...variables, pendingRow]);
      }
      return null;
    } catch (e: any) {
      alert(`File upload failed: ${e}`);
      return null;
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
      alert(`File selection failed: ${e}`);
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
    } catch (e) {
      alert(`Download failed: ${e}`);
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
    <>
      <table className="variables-table">
        <thead>
          <tr>
            <th style={{ width: '25%' }}>Name</th>
            <th style={{ width: '20%' }}>Type</th>
            <th>Value</th>
            <th style={{ width: '40px', textAlign: 'center' }}></th>
          </tr>
        </thead>
        <tbody>
          {variables.map((v, idx) => (
            <tr key={idx}>
              <td>
                <input
                  type="text"
                  className="var-input"
                  value={v.name}
                  onChange={(e) => handleChange(idx, 'name', e.target.value)}
                  placeholder="Variable name"
                  readOnly={readOnlyNames && !v.isNew}
                  style={{ backgroundColor: readOnlyNames && !v.isNew ? '#f8fafc' : '#ffffff' }}
                  autoCapitalize="off"
                  autoComplete="off"
                  spellCheck={false}
                />
              </td>
              <td>
                <select
                  className="var-select"
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
              </td>
              <td>
                {v.type === 'String' && (
                  <input
                    type="text"
                    className="var-input"
                    value={v.value as string}
                    onChange={(e) => handleChange(idx, 'value', e.target.value)}
                    placeholder="String value"
                    autoCapitalize="off"
                    autoComplete="off"
                    spellCheck={false}
                  />
                )}
                {v.type === 'Number' && (
                  <input
                    type="number"
                    className="var-input"
                    value={v.value as number}
                    onChange={(e) => handleChange(idx, 'value', parseFloat(e.target.value))}
                    placeholder="Number value"
                  />
                )}
                {v.type === 'Boolean' && (
                  <input
                    type="checkbox"
                    className="var-checkbox"
                    checked={v.value as boolean}
                    onChange={(e) => handleChange(idx, 'value', e.target.checked)}
                  />
                )}
                {v.type === 'Object' && (
                  <textarea
                    className="vars-textarea"
                    value={v.value as string}
                    onChange={(e) => handleChange(idx, 'value', e.target.value)}
                    rows={2}
                    spellCheck={false}
                    style={{ width: '100%', resize: 'vertical' }}
                  />
                )}
                {v.type === 'Null' && (
                  <span style={{ color: '#94a3b8', fontStyle: 'italic', fontSize: '0.85rem' }}>null</span>
                )}
                {v.type === 'File' && v.pendingFilePath && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }} className="file-pending-row">
                    <span className="file-badge">
                      <Paperclip size={12} />
                      {(v.value as any)?.filename || v.pendingFilePath.split('/').pop()}
                    </span>
                    <span className="pending-text">(Pending upload...)</span>
                  </div>
                )}
                {v.type === 'File' && !v.pendingFilePath && !isFileReference(v.value) && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <button
                      className="button"
                      onClick={() => handlePickFileForRow(idx)}
                      style={{
                        background: '#f1f5f9',
                        color: '#334155',
                        border: '1px solid #cbd5e1',
                        display: 'flex',
                        alignItems: 'center',
                        gap: '6px',
                        fontSize: '0.85rem',
                        padding: '4px 10px',
                        cursor: 'pointer',
                      }}
                    >
                      <Paperclip size={14} /> Choose File…
                    </button>
                    <span style={{ color: '#94a3b8', fontStyle: 'italic', fontSize: '0.85rem' }}>
                      No file selected
                    </span>
                  </div>
                )}
                {v.type === 'File' && !v.pendingFilePath && isFileReference(v.value) && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <div
                      onClick={() => {
                        if (instanceId) {
                          handleDownloadFile(v.name, (v.value as FileReference).filename);
                        }
                      }}
                      style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: instanceId ? 'pointer' : 'default', color: instanceId ? '#0f172a' : 'inherit' }}
                      title={instanceId ? "Download file" : ""}
                      className="file-download-trigger"
                    >
                      <Paperclip size={14} />
                      <span style={{ fontWeight: 500 }}>{(v.value as FileReference)?.filename}</span>
                    </div>
                    <span style={{ color: '#94a3b8', fontSize: '0.8rem' }}>
                      ({formatFileSize((v.value as FileReference)?.size_bytes || 0)})
                    </span>
                    {instanceId && (
                      <button 
                        onClick={() => handleDownloadFile(v.name, (v.value as FileReference).filename)}
                        style={{ marginLeft: 'auto', background: 'none', border: 'none', color: '#3b82f6', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: '4px' }}
                      >
                        <Download size={14} /> Download
                      </button>
                    )}
                  </div>
                )}
              </td>
              <td style={{ textAlign: 'center' }}>
                <button
                  className="button"
                  style={{ background: 'transparent', color: '#ef4444', border: 'none', padding: '4px', cursor: 'pointer' }}
                  onClick={() => handleRemove(idx)}
                  title="Delete Variable"
                >
                  <Trash size={16} />
                </button>
              </td>
            </tr>
          ))}
          {variables.length === 0 && (
            <tr>
              <td colSpan={4} style={{ textAlign: 'center', color: '#64748b', padding: '16px' }}>
                No variables configured.
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <div style={{ marginTop: 12, display: 'flex', justifyContent: 'flex-start', gap: '8px' }}>
        <button
          className="button"
          onClick={handleAdd}
          style={{ background: '#f1f5f9', color: '#334155', border: '1px solid #cbd5e1' }}
        >
          + Add Variable
        </button>
        {(instanceId || allowPendingFiles) && (
          <button
            className="button"
            onClick={handleUploadFile}
            style={{ background: '#f1f5f9', color: '#334155', border: '1px solid #cbd5e1', display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <Paperclip size={16} /> Attach File
          </button>
        )}
      </div>
    </>
  );
}

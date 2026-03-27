import { useState, useEffect, useCallback } from 'react';
import { listInstances, getInstanceDetails, getPendingTasks, updateInstanceVariables, getDefinitionXml, deleteInstance, type ProcessInstance, type PendingUserTask } from './lib/tauri';
import { InstanceViewer } from './InstanceViewer';
import { RefreshCw, Activity, CheckCircle, Clock, Trash } from 'lucide-react';

// Helper to render the instance state as a readable string
function stateLabel(state: ProcessInstance['state']): string {
  if (state === 'Running') return 'Running';
  if (state === 'Completed') return 'Completed';
  if (typeof state === 'object' && 'WaitingOnUserTask' in state) return 'Waiting on User Task';
  return String(state);
}

// Helper to pick a CSS class for the state badge
function stateBadgeClass(state: ProcessInstance['state']): string {
  if (state === 'Running') return 'state-badge state-running';
  if (state === 'Completed') return 'state-badge state-completed';
  return 'state-badge state-waiting';
}

export function Instances({ selectedInstanceId }: { selectedInstanceId?: string | null }) {
  const [instances, setInstances] = useState<ProcessInstance[]>([]);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ProcessInstance | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [pendingTasks, setPendingTasks] = useState<PendingUserTask[]>([]);
  const [variables, setVariables] = useState<VariableRow[]>([]);
  const [deletedKeys, setDeletedKeys] = useState<Set<string>>(new Set());

  // Type definition for variable rows
  type VarType = 'String' | 'Number' | 'Boolean' | 'Object' | 'Null';
  interface VariableRow {
    name: string;
    type: VarType;
    value: any;
    isNew?: boolean;
  }

  const parseVariables = (vars: Record<string, unknown>): VariableRow[] => {
    return Object.entries(vars)
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([name, val]) => {
      let type: VarType = 'String';
      if (val === null) type = 'Null';
      else if (typeof val === 'boolean') type = 'Boolean';
      else if (typeof val === 'number') type = 'Number';
      else if (typeof val === 'object') type = 'Object';
      
      return { 
        name, 
        type, 
        value: type === 'Object' ? JSON.stringify(val, null, 2) : val 
      };
    });
  };
  const [definitionXml, setDefinitionXml] = useState<string | null>(null);
  const [showNodeDetails, setShowNodeDetails] = useState(false);

  const fetchInstances = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await listInstances();
      setInstances(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchInstances();
  }, [fetchInstances]);

  const handleSelect = useCallback(async (inst: ProcessInstance) => {
    setDetailLoading(true);
    setDefinitionXml(null);
    setShowNodeDetails(false);
    try {
      const details = await getInstanceDetails(inst.id);
      setSelected(details);
      
      try {
        const xml = await getDefinitionXml(details.definition_key);
        setDefinitionXml(xml);
      } catch (xmlError) {
        console.error("Failed to fetch layout XML:", xmlError);
      }

      setVariables(parseVariables(details.variables));
      // If waiting on user task, fetch pending tasks to show info
      if (typeof details.state === 'object' && 'WaitingOnUserTask' in details.state) {
        const tasks = await getPendingTasks();
        setPendingTasks(tasks.filter(t => t.instance_id === details.id));
      } else {
        setPendingTasks([]);
      }
    } catch (e) {
      setSelected(inst);
      setVariables(parseVariables(inst.variables));
      setPendingTasks([]);
    } finally {
      setDetailLoading(false);
    }
  }, []);

  const handleVariableChange = (index: number, field: keyof VariableRow, newValue: any) => {
    const updated = [...variables];
    const row = updated[index];
    
    if (field === 'type') {
      row.type = newValue as VarType;
      if (row.type === 'String') row.value = '';
      else if (row.type === 'Number') row.value = 0;
      else if (row.type === 'Boolean') row.value = false;
      else if (row.type === 'Null') row.value = null;
      else if (row.type === 'Object') row.value = '{}';
    } else {
      (row[field] as any) = newValue;
    }
    setVariables(updated);
  };

  const handleAddVariable = () => {
    setVariables([...variables, { name: '', type: 'String', value: '', isNew: true }]);
  };

  const handleRemoveVariable = (index: number) => {
    const updated = [...variables];
    const removed = updated.splice(index, 1)[0];
    
    // Track removed keys that existed on the backend to instruct deletion
    if (!removed.isNew && removed.name.trim()) {
      const newDeleted = new Set(deletedKeys);
      newDeleted.add(removed.name);
      setDeletedKeys(newDeleted);
    }
    
    setVariables(updated);
  };

  const handleSaveVariables = async () => {
    if (!selected) return;
    try {
      const varsToSave: Record<string, unknown> = {};
      
      // Include explicitly deleted keys as null
      for (const key of deletedKeys) {
        varsToSave[key] = null;
      }
      
      for (const v of variables) {
        if (!v.name.trim()) continue; // skip unnamed variables
        
        if (v.type === 'Object') {
          try {
            varsToSave[v.name] = JSON.parse(v.value as string);
          } catch (e) {
            alert(`Invalid JSON for variable '${v.name}'`);
            return;
          }
        } else if (v.type === 'Number') {
          const num = Number(v.value);
          if (isNaN(num)) {
            alert(`Invalid number for variable '${v.name}'`);
            return;
          }
          varsToSave[v.name] = num;
        } else if (v.type === 'Boolean') {
          varsToSave[v.name] = Boolean(v.value);
        } else if (v.type === 'Null') {
          varsToSave[v.name] = null;
        } else {
          varsToSave[v.name] = v.value;
        }
      }

      await updateInstanceVariables(selected.id, varsToSave);
      alert('Variables saved successfully.');
      // Refresh the detail view
      const updated = await getInstanceDetails(selected.id);
      setSelected(updated);
      setVariables(parseVariables(updated.variables));
      setDeletedKeys(new Set());
    } catch (e) {
      alert('Error saving variables: ' + e);
    }
  };

  const handleClose = () => {
    setSelected(null);
    setPendingTasks([]);
    setDefinitionXml(null);
    setShowNodeDetails(false);
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    if (!window.confirm("Are you sure you want to delete this process instance?")) return;
    try {
      await deleteInstance(id);
      if (selected?.id === id) {
        handleClose();
      }
      fetchInstances();
    } catch (err) {
      alert("Failed to delete instance: " + err);
    }
  };

  useEffect(() => {
    if (selectedInstanceId && instances.length > 0 && (!selected || selected.id !== selectedInstanceId)) {
      const inst = instances.find(i => i.id === selectedInstanceId);
      if (inst) {
        handleSelect(inst);
      }
    }
  }, [selectedInstanceId, instances, selected, handleSelect]);

  return (
    <div>
      <h2>Instances</h2>
      <div className="header-actions">
        <button className="button" onClick={fetchInstances} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><RefreshCw size={16} /> Refresh</button>
      </div>

      {loading && <div style={{ margin: 20 }}>Loading instances...</div>}
      {error && <div style={{ margin: 20, color: '#dc2626' }}>Error: {error}</div>}
      {!loading && !error && instances.length === 0 && (
        <div style={{ margin: 20 }}>No instances found.</div>
      )}

      {instances.map(inst => (
        <div
          key={inst.id}
          className="card"
          style={{ cursor: 'pointer' }}
          onClick={() => handleSelect(inst)}
        >
          <div className="card-title">
            <span className={stateBadgeClass(inst.state)}>
              {inst.state === 'Running' && <Activity size={12} style={{marginRight: 4}} />}
              {inst.state === 'Completed' && <CheckCircle size={12} style={{marginRight: 4}} />}
              {typeof inst.state === 'object' && <Clock size={12} style={{marginRight: 4}} />}
              {stateLabel(inst.state)}
            </span>
            {' '}Instance: {inst.id.substring(0, 8)}…
            <button
              style={{ marginLeft: 'auto', background: 'transparent', color: '#ef4444', border: 'none', padding: '4px', cursor: 'pointer' }}
              onClick={(e) => handleDelete(e, inst.id)}
              title="Delete Instance"
            >
              <Trash size={16} />
            </button>
          </div>
          <div style={{ marginTop: '6px', fontSize: '0.9rem', color: '#1e293b', fontWeight: 500 }}>
            Business Key: {inst.business_key}
          </div>
          <div style={{ marginTop: '4px' }}>Definition: {inst.definition_key.substring(0, 8)}…</div>
          <div>Current Node: {inst.current_node}</div>
        </div>
      ))}

      {/* Detail view */}
      {selected && (
        <div className="instance-detail card">
          <div className="card-title" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span>Instance Details: {selected.id.substring(0, 8)}…</span>
            <div style={{ display: 'flex', gap: '8px' }}>
              <button className="button" onClick={(e) => handleDelete(e, selected.id)} style={{ background: '#ef4444', fontSize: '0.85rem', padding: '4px 10px', display: 'flex', alignItems: 'center', gap: '4px' }}>
                <Trash size={14} /> Delete
              </button>
              <button className="button" onClick={handleClose} style={{ background: '#64748b', fontSize: '0.85rem', padding: '4px 10px' }}>Close</button>
            </div>
          </div>

          {detailLoading ? (
            <div>Loading details...</div>
          ) : (
            <>
              <div style={{ marginBottom: 12 }}>
                <strong>State:</strong>{' '}
                <span className={stateBadgeClass(selected.state)}>{stateLabel(selected.state)}</span>
              </div>
              <div style={{ marginBottom: 12 }}>
                <strong>Business Key:</strong> {selected.business_key}
              </div>
              <div style={{ marginBottom: 12 }}>
                <strong>Definition:</strong> {selected.definition_key}
              </div>

              {definitionXml && (
                <div style={{ marginBottom: 16 }}>
                  <strong>Process Workflow:</strong>
                  <InstanceViewer 
                    xml={definitionXml} 
                    activeNodeId={selected.current_node} 
                    onNodeClick={() => setShowNodeDetails((prev) => !prev)} 
                  />
                  {!showNodeDetails && (
                    <div style={{ fontSize: '0.85rem', color: '#64748b' }}>
                      Click on the highlighted active node ({selected.current_node}) to view variables and state details.
                    </div>
                  )}
                </div>
              )}

              {(!definitionXml || showNodeDetails) && (
                <div style={{ padding: '12px', backgroundColor: '#f8fafc', border: '1px solid #e2e8f0', borderRadius: '4px' }}>
                  <div style={{ marginBottom: 12 }}>
                    <strong>Current Node:</strong> {selected.current_node}
                  </div>

              {/* Pending user task info */}
              {pendingTasks.length > 0 && (
                <div style={{ marginBottom: 12 }}>
                  <strong>Pending User Task:</strong>
                  {pendingTasks.map(task => (
                    <div key={task.task_id} style={{ marginLeft: 12, marginTop: 4 }}>
                      Node: {task.node_id} · Assignee: {task.assignee}
                    </div>
                  ))}
                </div>
              )}

              {/* Audit log */}
              <div style={{ marginBottom: 12 }}>
                <strong>Audit Log:</strong>
                <ul className="audit-log">
                  {selected.audit_log.map((entry, i) => (
                    <li key={i}>{entry}</li>
                  ))}
                </ul>
              </div>

              {/* Editable variables */}
              <div style={{ marginTop: 16 }}>
                <strong>Variables:</strong>
                
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
                            onChange={(e) => handleVariableChange(idx, 'name', e.target.value)}
                            placeholder="Variable name"
                            readOnly={!v.isNew} 
                            style={{ backgroundColor: !v.isNew ? '#f8fafc' : '#ffffff' }}
                          />
                        </td>
                        <td>
                          <select 
                            className="var-select" 
                            value={v.type} 
                            onChange={(e) => handleVariableChange(idx, 'type', e.target.value)}
                          >
                            <option value="String">String</option>
                            <option value="Number">Number</option>
                            <option value="Boolean">Boolean</option>
                            <option value="Object">Object</option>
                            <option value="Null">Null</option>
                          </select>
                        </td>
                        <td>
                          {v.type === 'String' && (
                            <input 
                              type="text" 
                              className="var-input" 
                              value={v.value as string} 
                              onChange={(e) => handleVariableChange(idx, 'value', e.target.value)} 
                              placeholder="String value"
                            />
                          )}
                          {v.type === 'Number' && (
                            <input 
                              type="number" 
                              className="var-input" 
                              value={v.value as number} 
                              onChange={(e) => handleVariableChange(idx, 'value', parseFloat(e.target.value))} 
                              placeholder="Number value"
                            />
                          )}
                          {v.type === 'Boolean' && (
                            <input 
                              type="checkbox" 
                              className="var-checkbox"
                              checked={v.value as boolean} 
                              onChange={(e) => handleVariableChange(idx, 'value', e.target.checked)} 
                            />
                          )}
                          {v.type === 'Object' && (
                            <textarea 
                              className="vars-textarea" 
                              value={v.value as string} 
                              onChange={(e) => handleVariableChange(idx, 'value', e.target.value)} 
                              rows={2}
                              spellCheck={false}
                              style={{ width: '100%', resize: 'vertical' }}
                            />
                          )}
                          {v.type === 'Null' && (
                            <span style={{ color: '#94a3b8', fontStyle: 'italic', fontSize: '0.85rem' }}>null</span>
                          )}
                        </td>
                        <td style={{ textAlign: 'center' }}>
                          <button 
                            className="button" 
                            style={{ background: 'transparent', color: '#ef4444', border: 'none', padding: '4px', cursor: 'pointer' }}
                            onClick={() => handleRemoveVariable(idx)} 
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
                <div style={{ marginTop: 12, display: 'flex', justifyContent: 'space-between' }}>
                  <button className="button" onClick={handleAddVariable} style={{ background: '#f1f5f9', color: '#334155', border: '1px solid #cbd5e1' }}>
                    + Add Variable
                  </button>
                  <button className="button save-vars-btn" onClick={handleSaveVariables}>
                    Save Variables
                  </button>
                </div>
              </div>
              </div> /* End of detail toggle section */
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

import { useState, useEffect, useCallback } from 'react';
import { listInstances, getInstanceDetails, getPendingTasks, getPendingServiceTasks, updateInstanceVariables, getDefinitionXml, deleteInstance, listDefinitions, type ProcessInstance, type PendingUserTask, type PendingServiceTask, type DefinitionInfo } from './lib/tauri';
import { InstanceViewer } from './InstanceViewer';
import { RefreshCw, Activity, CheckCircle, Clock, Trash, FileCode2, Network, ScrollText } from 'lucide-react';
import { VariableEditor, type VariableRow, parseVariables, serializeVariables } from './VariableEditor';
import { HistoryTimeline } from './HistoryTimeline';
import { ErrorBoundary } from './ErrorBoundary';

// Helper to render the instance state as a readable string
function stateLabel(state: ProcessInstance['state']): string {
  if (state === 'Running') return 'Running';
  if (state === 'Completed') return 'Completed';
  if (typeof state === 'object' && 'WaitingOnUserTask' in state) return 'Wait: User Task';
  if (typeof state === 'object' && 'WaitingOnServiceTask' in state) return 'Wait: Service Task';
  if (typeof state === 'object' && 'WaitingOnTimer' in state) return 'Wait: Timer';
  if (typeof state === 'object' && 'WaitingOnMessage' in state) return 'Wait: Message';
  return String(state);
}

// Helper to pick a CSS class for the state badge
function stateBadgeClass(state: ProcessInstance['state']): string {
  if (state === 'Running') return 'state-badge state-running';
  if (state === 'Completed') return 'state-badge state-completed';
  if (typeof state === 'object' && 'WaitingOnServiceTask' in state) return 'state-badge state-waiting state-service-task';
  if (typeof state === 'object' && 'WaitingOnTimer' in state) return 'state-badge state-waiting';
  if (typeof state === 'object' && 'WaitingOnMessage' in state) return 'state-badge state-waiting';
  return 'state-badge state-waiting';
}

function groupInstances(instances: ProcessInstance[], definitions: DefinitionInfo[]) {
  const defMap = new Map<string, DefinitionInfo>();
  for (const d of definitions) defMap.set(d.key, d);

  const groups = new Map<string, ProcessInstance[]>();
  const unknownGroup: ProcessInstance[] = [];

  for (const inst of instances) {
    const def = defMap.get(inst.definition_key);
    if (def) {
      const arr = groups.get(def.bpmn_id) || [];
      arr.push(inst);
      groups.set(def.bpmn_id, arr);
    } else {
      unknownGroup.push(inst);
    }
  }

  // Sort each group so running processes are at the top, then by instance id roughly
  for (const [, insts] of groups) {
    insts.sort((a, b) => {
      if (a.state === 'Completed' && b.state !== 'Completed') return 1;
      if (a.state !== 'Completed' && b.state === 'Completed') return -1;
      return a.id.localeCompare(b.id);
    });
  }

  return { groups, unknownGroup, defMap };
}

export function Instances({ selectedInstanceId }: { selectedInstanceId?: string | null }) {
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ProcessInstance | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [pendingTasks, setPendingTasks] = useState<PendingUserTask[]>([]);
  const [pendingServiceTasks, setPendingServiceTasks] = useState<PendingServiceTask[]>([]);
  const [variables, setVariables] = useState<VariableRow[]>([]);
  const [deletedKeys, setDeletedKeys] = useState<Set<string>>(new Set());
  const [historyRefreshTrigger, setHistoryRefreshTrigger] = useState(0);

  // Variables state is typed using VariableRow from VariableEditor
  const [definitionXml, setDefinitionXml] = useState<string | null>(null);
  const [showNodeDetails, setShowNodeDetails] = useState(false);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [instList, defList] = await Promise.all([listInstances(), listDefinitions()]);
      setInstances(instList);
      setDefinitions(defList);
      setHistoryRefreshTrigger(prev => prev + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleSelect = useCallback(async (inst: ProcessInstance) => {
    setDetailLoading(true);
    setDefinitionXml(null);
    setShowNodeDetails(false);
    setHistoryRefreshTrigger(prev => prev + 1);
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
    } catch (e) {
      setSelected(inst);
      setVariables(parseVariables(inst.variables));
      setPendingTasks([]);
      setPendingServiceTasks([]);
    } finally {
      setDetailLoading(false);
    }
  }, []);

  const handleSaveVariables = async () => {
    if (!selected) return;
    const varsToSave = serializeVariables(variables, deletedKeys);
    if (varsToSave === null) return;

    try {
      await updateInstanceVariables(selected.id, varsToSave);
      alert('Variables saved successfully.');
      const updated = await getInstanceDetails(selected.id);
      setSelected(updated);
      setVariables(parseVariables(updated.variables));
      setDeletedKeys(new Set());
      setHistoryRefreshTrigger(prev => prev + 1);
    } catch (e) {
      alert('Error saving variables: ' + e);
    }
  };

  const handleClose = () => {
    setSelected(null);
    setPendingTasks([]);
    setPendingServiceTasks([]);
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
      fetchData();
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

  const { groups, unknownGroup, defMap } = groupInstances(instances, definitions);

  return (
    <div>
      <h2>Instances</h2>
      <div className="header-actions">
        <button className="button" onClick={fetchData} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><RefreshCw size={16} /> Refresh</button>
      </div>

      {loading && <div style={{ margin: 20 }}>Loading instances...</div>}
      {error && <div style={{ margin: 20, color: '#dc2626' }}>Error: {error}</div>}
      {!loading && !error && instances.length === 0 && (
        <div style={{ margin: 20 }}>No instances found.</div>
      )}

      <div style={{ padding: '0 20px' }}>
        {[...groups.entries()].map(([bpmnId, groupInstances]) => {
          const activeCount = groupInstances.filter(i => i.state !== 'Completed').length;
          
          return (
            <div key={bpmnId} className="process-group-card" style={{ margin: '20px 0' }}>
              <div className="process-group-header">
                <div className="process-title" style={{ fontSize: '1.2rem' }}>
                  <FileCode2 size={20} color="#2563eb" /> {bpmnId}
                </div>
                <div className="process-stats">
                  <span className="stat-pill">{groupInstances.length} total</span>
                  {activeCount > 0 && <span className="stat-pill highlight">{activeCount} active</span>}
                </div>
              </div>
              <div style={{ padding: '16px', background: '#f8fafc', display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {groupInstances.map(inst => {
                  const def = defMap.get(inst.definition_key);
                  const varCount = Object.keys(inst.variables || {}).length;
                  const logCount = inst.audit_log?.length || 0;
                  
                  return (
                    <div
                      key={inst.id}
                      className="instance-list-item"
                      style={{ margin: 0 }}
                      onClick={() => handleSelect(inst)}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', gap: '16px', flex: 1 }}>
                        <div style={{ minWidth: '130px' }}>
                          <span className={stateBadgeClass(inst.state)} style={{ width: '100%', textAlign: 'center', boxSizing: 'border-box' }}>
                            {inst.state === 'Running' && <Activity size={10} style={{marginRight: 4}} />}
                            {inst.state === 'Completed' && <CheckCircle size={10} style={{marginRight: 4}} />}
                            {typeof inst.state === 'object' && <Clock size={10} style={{marginRight: 4}} />}
                            {stateLabel(inst.state)}
                          </span>
                        </div>
                        <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                          <span style={{ fontWeight: 600, color: '#1e293b' }}>
                            {inst.business_key || inst.id.substring(0, 8)} 
                            <span style={{ fontWeight: 'normal', color: '#94a3b8', marginLeft: '8px' }}>(#{inst.id.substring(0, 8)})</span>
                          </span>
                          <span style={{ fontSize: '0.8rem', color: '#64748b' }}>
                            <Network size={12} style={{ display: 'inline', verticalAlign: 'text-bottom' }} /> {inst.current_node}
                          </span>
                        </div>
                      </div>

                      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                        {def && (
                          <span className={`version-pill ${def.is_latest ? 'latest' : 'older'}`} title={`Definition Key: ${def.key}`}>
                            v{def.version}
                          </span>
                        )}
                        <span className="stat-pill" title={`${varCount} variables active`}><ScrollText size={12} style={{ display: 'inline', verticalAlign: 'text-bottom' }} /> {varCount}</span>
                        <span className="stat-pill" title={`${logCount} audit entries`}><Activity size={12} style={{ display: 'inline', verticalAlign: 'text-bottom' }} /> {logCount}</span>
                        
                        <button
                          style={{ background: 'transparent', color: '#ef4444', border: '1px solid transparent', borderRadius: '4px', padding: '6px', cursor: 'pointer', transition: 'all 0.2s', marginLeft: '8px' }}
                          onClick={(e) => handleDelete(e, inst.id)}
                          title="Delete Instance"
                          onMouseEnter={(e) => { e.currentTarget.style.background = '#fee2e2'; e.currentTarget.style.borderColor = '#fca5a5'; }}
                          onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.borderColor = 'transparent'; }}
                        >
                          <Trash size={16} />
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}

        {unknownGroup.length > 0 && (
          <div className="process-group-card" style={{ margin: '20px 0', opacity: 0.8 }}>
             <div className="process-group-header">
                <div className="process-title" style={{ fontSize: '1.2rem', color: '#64748b' }}>
                  Unknown Definitions
                </div>
              </div>
              <div style={{ padding: '16px', background: '#f8fafc', display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {unknownGroup.map(inst => (
                   <div key={inst.id} className="instance-list-item" style={{ margin: 0 }} onClick={() => handleSelect(inst)}>
                     <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
                       <span className={stateBadgeClass(inst.state)}>{stateLabel(inst.state)}</span>
                       <span style={{ fontWeight: 600 }}>{inst.business_key || inst.id.substring(0, 8)}</span>
                     </div>
                     <div>{inst.definition_key.substring(0, 8)}…</div>
                   </div>
                ))}
              </div>
          </div>
        )}
      </div>

      {/* Detail view overlay */}
      {selected && (
        <div className="vars-dialog-overlay" style={{ zIndex: 50, padding: '20px', alignItems: 'flex-start', overflowY: 'auto' }}>
          <div className="instance-detail card" style={{ maxWidth: '900px', width: '100%', margin: '0 auto', background: 'white' }}>
            <div className="card-title" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid #e2e8f0', paddingBottom: '12px', marginBottom: '16px' }}>
              <span style={{ fontSize: '1.2rem' }}>Instance: {selected.id.substring(0, 8)}…</span>
              <div style={{ display: 'flex', gap: '8px' }}>
                <button className="button" onClick={(e) => handleDelete(e, selected.id)} style={{ background: '#ef4444', fontSize: '0.85rem', padding: '6px 12px', display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <Trash size={14} /> Delete
                </button>
                <button className="button" onClick={handleClose} style={{ background: '#64748b', fontSize: '0.85rem', padding: '6px 16px' }}>Close View</button>
              </div>
            </div>

            {detailLoading ? (
              <div style={{ padding: '20px', textAlign: 'center', color: '#64748b' }}>Loading instance context...</div>
            ) : (
              <>
                <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: '16px', marginBottom: '20px' }}>
                  <div style={{ padding: '12px', background: '#f8fafc', borderRadius: '8px', border: '1px solid #e2e8f0' }}>
                    <div style={{ fontSize: '0.75rem', color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.5px', marginBottom: '4px' }}>State</div>
                    <span className={stateBadgeClass(selected.state)}>{stateLabel(selected.state)}</span>
                  </div>
                  <div style={{ padding: '12px', background: '#f8fafc', borderRadius: '8px', border: '1px solid #e2e8f0' }}>
                    <div style={{ fontSize: '0.75rem', color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.5px', marginBottom: '4px' }}>Business Key</div>
                    <span style={{ fontWeight: 600 }}>{selected.business_key || 'None'}</span>
                  </div>
                  <div style={{ padding: '12px', background: '#f8fafc', borderRadius: '8px', border: '1px solid #e2e8f0' }}>
                    <div style={{ fontSize: '0.75rem', color: '#64748b', textTransform: 'uppercase', letterSpacing: '0.5px', marginBottom: '4px' }}>Process ID</div>
                    <span style={{ fontWeight: 600, fontFamily: 'monospace' }}>{defMap.get(selected.definition_key)?.bpmn_id || selected.definition_key.substring(0, 8)}</span>
                    {defMap.get(selected.definition_key) && (
                      <span className="version-pill older" style={{ marginLeft: '8px' }}>v{defMap.get(selected.definition_key)!.version}</span>
                    )}
                  </div>
                </div>

                {definitionXml && (
                  <div style={{ marginBottom: 24 }}>
                    <h3 style={{ fontSize: '1rem', color: '#1e293b', marginBottom: '12px' }}>Process Workflow</h3>
                    <ErrorBoundary>
                      <InstanceViewer 
                        xml={definitionXml} 
                        activeNodeId={selected.current_node} 
                        onNodeClick={() => setShowNodeDetails((prev) => !prev)} 
                      />
                    </ErrorBoundary>
                    {!showNodeDetails && (
                      <div style={{ fontSize: '0.85rem', color: '#64748b', marginTop: '8px' }}>
                        Click on the highlighted active node ({selected.current_node}) to view variables and state details.
                      </div>
                    )}
                  </div>
                )}

                {(!definitionXml || showNodeDetails) && (
                  <ErrorBoundary>
                  <div style={{ background: '#f1f5f9', border: '1px solid #cbd5e1', borderRadius: '8px', padding: '20px' }}>
                    <h3 style={{ fontSize: '1rem', color: '#1e293b', margin: '0 0 16px 0', borderBottom: '1px solid #e2e8f0', paddingBottom: '8px' }}>
                      Node Context: <span style={{ fontFamily: 'monospace', color: '#2563eb' }}>{selected?.current_node || 'Unknown'}</span>
                    </h3>

                    {/* Pending user task info */}
                    {pendingTasks?.length > 0 && (
                      <div style={{ marginBottom: 16, background: '#fff', border: '1px solid #e2e8f0', padding: '12px', borderRadius: '6px' }}>
                        <strong style={{ color: '#0f172a' }}>Assigned User Tasks:</strong>
                        {pendingTasks.map(task => (
                          <div key={task.task_id} style={{ marginTop: 8, padding: '8px', background: '#f8fafc', borderRadius: '4px' }}>
                            <span style={{ fontWeight: 500, color: '#334155' }}>Node: {task.node_id}</span>
                            <br/><span style={{ fontSize: '0.9rem', color: '#64748b' }}>Assignee: {task.assignee || 'Unassigned'}</span>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Pending service task info */}
                    {pendingServiceTasks?.length > 0 && (
                      <div style={{ marginBottom: 16, background: '#fff', border: '1px solid #e2e8f0', padding: '12px', borderRadius: '6px' }}>
                        <strong style={{ color: '#0f172a' }}>Pending Service Tasks (Workers):</strong>
                        {pendingServiceTasks.map(task => (
                          <div key={task?.id || Math.random().toString()} style={{ marginTop: 8, padding: '8px', background: '#f8fafc', borderRadius: '4px' }}>
                            <span style={{ fontWeight: 500, color: '#334155' }}>Node: {task?.node_id}</span>
                            <br/>Topic: <span style={{ fontWeight: 600, color: '#0369a1' }}>{task?.topic}</span>
                            <div style={{ fontSize: '0.85em', color: '#64748b', marginTop: '4px' }}>
                              Worker ID: {task?.worker_id || 'Unlocked'} · Remaining Retries: {task?.retries}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Execution History Timeline */}
                    <div style={{ marginBottom: 20 }}>
                      <strong style={{ display: 'block', marginBottom: '12px', color: '#0f172a' }}>Execution History:</strong>
                      <div style={{ background: '#fff', borderRadius: '6px', border: '1px solid #e2e8f0', padding: '12px' }}>
                        <HistoryTimeline instanceId={selected.id} refreshTrigger={historyRefreshTrigger} />
                      </div>
                    </div>

                    {/* Editable variables */}
                    <div style={{ marginTop: 16 }}>
                      <strong style={{ display: 'block', marginBottom: '12px', color: '#0f172a' }}>Variables:</strong>
                      <div style={{ background: '#fff', borderRadius: '6px', border: '1px solid #e2e8f0', padding: '12px' }}>
                        <VariableEditor
                          variables={variables}
                          onChange={setVariables}
                          readOnlyNames={true}
                          deletedKeys={deletedKeys}
                          onDeletedKeysChange={setDeletedKeys}
                          instanceId={selected.id}
                          onVariablesRefreshRequest={() => handleSelect(selected)}
                        />
                        <div style={{ marginTop: 16, display: 'flex', justifyContent: 'flex-end', borderTop: '1px solid #e2e8f0', paddingTop: '16px' }}>
                          <button className="button save-vars-btn" onClick={handleSaveVariables} style={{ padding: '8px 24px', fontSize: '0.95rem' }}>
                            Save Variables
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                  </ErrorBoundary>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

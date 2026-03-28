import { useState, useEffect, useCallback } from 'react';
import { listInstances, getInstanceDetails, getPendingTasks, getPendingServiceTasks, updateInstanceVariables, getDefinitionXml, deleteInstance, type ProcessInstance, type PendingUserTask, type PendingServiceTask } from './lib/tauri';
import { InstanceViewer } from './InstanceViewer';
import { RefreshCw, Activity, CheckCircle, Clock, Trash } from 'lucide-react';
import { VariableEditor, type VariableRow, parseVariables, serializeVariables } from './VariableEditor';
import { HistoryTimeline } from './HistoryTimeline';
import { ErrorBoundary } from './ErrorBoundary';

// Helper to render the instance state as a readable string
function stateLabel(state: ProcessInstance['state']): string {
  if (state === 'Running') return 'Running';
  if (state === 'Completed') return 'Completed';
  if (typeof state === 'object' && 'WaitingOnUserTask' in state) return 'Waiting on User Task';
  if (typeof state === 'object' && 'WaitingOnServiceTask' in state) return 'Waiting on Service Task';
  return String(state);
}

// Helper to pick a CSS class for the state badge
function stateBadgeClass(state: ProcessInstance['state']): string {
  if (state === 'Running') return 'state-badge state-running';
  if (state === 'Completed') return 'state-badge state-completed';
  if (typeof state === 'object' && 'WaitingOnServiceTask' in state) return 'state-badge state-waiting state-service-task';
  return 'state-badge state-waiting';
}

export function Instances({ selectedInstanceId }: { selectedInstanceId?: string | null }) {
  const [instances, setInstances] = useState<ProcessInstance[]>([]);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ProcessInstance | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [pendingTasks, setPendingTasks] = useState<PendingUserTask[]>([]);
  const [pendingServiceTasks, setPendingServiceTasks] = useState<PendingServiceTask[]>([]);
  const [variables, setVariables] = useState<VariableRow[]>([]);
  const [deletedKeys, setDeletedKeys] = useState<Set<string>>(new Set());

  // Variables state is typed using VariableRow from VariableEditor
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
                  <ErrorBoundary>
                    <InstanceViewer 
                      xml={definitionXml} 
                      activeNodeId={selected.current_node} 
                      onNodeClick={() => setShowNodeDetails((prev) => !prev)} 
                    />
                  </ErrorBoundary>
                  {!showNodeDetails && (
                    <div style={{ fontSize: '0.85rem', color: '#64748b' }}>
                      Click on the highlighted active node ({selected.current_node}) to view variables and state details.
                    </div>
                  )}
                </div>
              )}

              {(!definitionXml || showNodeDetails) && (
                <ErrorBoundary>
                <div style={{ padding: '12px', backgroundColor: '#f8fafc', border: '1px solid #e2e8f0', borderRadius: '4px' }}>
                  <div style={{ marginBottom: 12 }}>
                    <strong>Current Node:</strong> {selected?.current_node || 'Unknown'}
                  </div>

                  {/* Pending user task info */}
                  {pendingTasks?.length > 0 && (
                    <div style={{ marginBottom: 12 }}>
                      <strong>Pending User Task:</strong>
                      {pendingTasks.map(task => (
                        <div key={task.task_id} style={{ marginLeft: 12, marginTop: 4 }}>
                          Node: {task.node_id} · Assignee: {task.assignee}
                        </div>
                      ))}
                    </div>
                  )}

                  {/* Pending service task info */}
                  {pendingServiceTasks?.length > 0 && (
                    <div style={{ marginBottom: 12 }}>
                      <strong>Pending Service Task:</strong>
                      {pendingServiceTasks.map(task => (
                        <div key={task?.id || Math.random().toString()} style={{ marginLeft: 12, marginTop: 4 }}>
                          Node: {task?.node_id} · Topic: <span style={{ fontWeight: 600 }}>{task?.topic}</span>
                          <br/>
                          <span style={{ fontSize: '0.85em', color: '#64748b' }}>
                            Worker: {task?.worker_id || 'Unlocked'} · Retries: {task?.retries}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}

                  {/* Execution History Timeline */}
                  <div style={{ marginBottom: 16 }}>
                    <strong>Execution History:</strong>
                    <HistoryTimeline instanceId={selected.id} />
                  </div>

                  {/* Editable variables */}
                  <div style={{ marginTop: 16 }}>
                    <strong>Variables:</strong>
                    <VariableEditor
                      variables={variables}
                      onChange={setVariables}
                      readOnlyNames={true}
                      deletedKeys={deletedKeys}
                      onDeletedKeysChange={setDeletedKeys}
                    />
                    <div style={{ marginTop: 12, display: 'flex', justifyContent: 'flex-end' }}>
                      <button className="button save-vars-btn" onClick={handleSaveVariables}>
                        Save Variables
                      </button>
                    </div>
                  </div>
                </div>
                </ErrorBoundary>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

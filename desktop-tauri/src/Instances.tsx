import { useState, useEffect, useCallback } from 'react';
import { listInstances, getInstanceDetails, getPendingTasks, type ProcessInstance, type PendingUserTask } from './lib/tauri';

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

export function Instances() {
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ProcessInstance | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [pendingTasks, setPendingTasks] = useState<PendingUserTask[]>([]);

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

  const handleSelect = async (inst: ProcessInstance) => {
    setDetailLoading(true);
    try {
      const details = await getInstanceDetails(inst.id);
      setSelected(details);
      // If waiting on user task, fetch pending tasks to show info
      if (typeof details.state === 'object' && 'WaitingOnUserTask' in details.state) {
        const tasks = await getPendingTasks();
        setPendingTasks(tasks.filter(t => t.instance_id === details.id));
      } else {
        setPendingTasks([]);
      }
    } catch (e) {
      setSelected(inst);
      setPendingTasks([]);
    } finally {
      setDetailLoading(false);
    }
  };

  const handleClose = () => {
    setSelected(null);
    setPendingTasks([]);
  };

  return (
    <div>
      <h2>Instances</h2>
      <div className="header-actions">
        <button className="button" onClick={fetchInstances}>Refresh</button>
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
            <span className={stateBadgeClass(inst.state)}>{stateLabel(inst.state)}</span>
            {' '}Instance: {inst.id.substring(0, 8)}…
          </div>
          <div>Definition: {inst.definition_id}</div>
          <div>Current Node: {inst.current_node}</div>
        </div>
      ))}

      {/* Detail view */}
      {selected && (
        <div className="instance-detail card">
          <div className="card-title" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span>Instance Details: {selected.id.substring(0, 8)}…</span>
            <button className="button" onClick={handleClose} style={{ background: '#64748b', fontSize: '0.85rem', padding: '4px 10px' }}>Close</button>
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
                <strong>Definition:</strong> {selected.definition_id}
              </div>
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

              {/* Variables */}
              <div>
                <strong>Variables:</strong>
                <pre className="variables-block">
                  {Object.keys(selected.variables).length > 0
                    ? JSON.stringify(selected.variables, null, 2)
                    : '(none)'}
                </pre>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

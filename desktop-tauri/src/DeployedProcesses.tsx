import { useState, useEffect, useCallback } from 'react';
import { save } from '@tauri-apps/api/dialog';
import { writeTextFile } from '@tauri-apps/api/fs';
import { listDefinitions, getDefinitionXml, listInstances, deleteDefinition, type DefinitionInfo, type ProcessInstance } from './lib/tauri';
import { RefreshCw, Eye, Download, Activity, Clock, Trash } from 'lucide-react';

export function DeployedProcesses({ onView, onViewInstance }: { onView: (xml: string) => void, onViewInstance?: (id: string) => void }) {
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [viewingId, setViewingId] = useState<string | null>(null);

  const fetchDefinitions = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [list, instList] = await Promise.all([listDefinitions(), listInstances()]);
      setDefinitions(list);
      setInstances(instList);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDefinitions();
  }, [fetchDefinitions]);

  const handleDownload = async (defId: string) => {
    setDownloading(defId);
    try {
      const xml = await getDefinitionXml(defId);
      // Use Tauri native save dialog + fs write
      const filePath = await save({
        defaultPath: `definition-${defId.substring(0, 8)}.bpmn`,
        filters: [{ name: 'BPMN', extensions: ['bpmn', 'xml'] }],
      });
      if (filePath) {
        await writeTextFile(filePath, xml);
      }
    } catch (e) {
      alert('Download failed: ' + e);
    } finally {
      setDownloading(null);
    }
  };

  const handleView = async (defId: string) => {
    setViewingId(defId);
    try {
      const xml = await getDefinitionXml(defId);
      onView(xml);
    } catch (e) {
      alert('Failed to load definition: ' + e);
    } finally {
      setViewingId(null);
    }
  };

  const handleDelete = async (defId: string) => {
    const relatedInstances = instances.filter(i => i.definition_key === defId);
    let cascade = false;
    
    if (relatedInstances.length > 0) {
      const msg = `This process definition has ${relatedInstances.length} associated instance(s). Deleting it will also permanently delete all associated instances.\n\nAre you sure you want to delete this definition AND its instances?`;
      if (!window.confirm(msg)) return;
      cascade = true;
    } else {
      if (!window.confirm("Are you sure you want to delete this process definition?")) return;
    }

    try {
      await deleteDefinition(defId, cascade);
      if (viewingId === defId) {
        setViewingId(null);
      }
      // Re-fetch standard lists
      fetchDefinitions();
    } catch (e) {
      alert('Delete failed: ' + e);
    }
  };

  return (
    <div>
      <h2>Deployed Processes</h2>
      <div className="header-actions">
        <button className="button" onClick={fetchDefinitions} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><RefreshCw size={16} /> Refresh</button>
      </div>

      {loading && <div style={{ margin: 20 }}>Loading definitions...</div>}
      {error && <div style={{ margin: 20, color: '#dc2626' }}>Error: {error}</div>}
      {!loading && !error && definitions.length === 0 && (
        <div style={{ margin: 20 }}>No deployed processes.</div>
      )}

      {definitions.map(def => (
        <div key={def.key} className="card">
          <div className="card-title">Definition: {def.bpmn_id}</div>
          <div className="def-card-meta">
            <span>Nodes: {def.node_count}</span>
          </div>
          <div className="def-card-actions">
            <button
              className="button"
              onClick={() => handleView(def.key)}
              disabled={viewingId === def.key}
              style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
            >
              <Eye size={16} /> {viewingId === def.key ? 'Loading...' : 'View in Modeler'}
            </button>
            <button
              className="button button-secondary"
              onClick={() => handleDownload(def.key)}
              disabled={downloading === def.key}
              style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
            >
              <Download size={16} /> {downloading === def.key ? 'Downloading...' : 'Download BPMN'}
            </button>
            <button
              className="button"
              onClick={() => handleDelete(def.key)}
              style={{ display: 'flex', alignItems: 'center', gap: '6px', background: '#ef4444', marginLeft: 'auto' }}
              title="Delete Definition"
            >
              <Trash size={16} /> Delete
            </button>
          </div>
          <div style={{ marginTop: 15, borderTop: '1px solid #e2e8f0', paddingTop: 10 }}>
            <h4 style={{ margin: '0 0 10px 0', fontSize: '0.9rem', color: '#475569' }}>Running Instances</h4>
            {(() => {
              const defInstances = instances.filter(i => i.definition_key === def.key && i.state !== 'Completed');
              if (defInstances.length === 0) {
                return <div style={{ fontSize: '0.85rem', color: '#64748b' }}>No running instances.</div>;
              }
              return (
                <ul style={{ margin: 0, padding: 0, listStyle: 'none' }}>
                  {defInstances.map(inst => (
                    <li key={inst.id} style={{ marginBottom: 6 }}>
                      <button 
                        className="button" 
                        style={{ padding: '4px 8px', fontSize: '0.85rem', width: '100%', textAlign: 'left', display: 'flex', justifyContent: 'space-between', alignItems: 'center', background: '#f8fafc', color: '#334155', border: '1px solid #cbd5e1' }}
                        onClick={() => onViewInstance?.(inst.id)}
                      >
                        <span style={{ display: 'flex', alignItems: 'center', gap: '4px', fontWeight: 500 }}>
                          {inst.state === 'Running' ? <Activity size={12} color="#10b981" /> : <Clock size={12} color="#f59e0b" />}
                          {inst.business_key}
                        </span>
                        <span style={{ color: '#64748b', fontSize: '0.75rem' }}>{inst.current_node}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              );
            })()}
          </div>
        </div>
      ))}
    </div>
  );
}

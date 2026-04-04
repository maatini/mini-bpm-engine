import { useState, useEffect, useCallback } from 'react';
import { save } from '@tauri-apps/api/dialog';
import { writeTextFile } from '@tauri-apps/api/fs';
import { listDefinitions, getDefinitionXml, listInstances, deleteDefinition, type DefinitionInfo, type ProcessInstance } from './lib/tauri';
import { RefreshCw, Eye, Download, Activity, Clock, Trash, ChevronDown, ChevronRight, FileCode2, Network, Key, Boxes } from 'lucide-react';

function groupByProcess(defs: DefinitionInfo[]): Map<string, DefinitionInfo[]> {
  const map = new Map<string, DefinitionInfo[]>();
  for (const d of defs) {
    const existing = map.get(d.bpmn_id) || [];
    existing.push(d);
    map.set(d.bpmn_id, existing);
  }
  for (const [, versions] of map) {
    versions.sort((a, b) => b.version - a.version);
  }
  return map;
}

export function DeployedProcesses({ onView, onViewInstance }: { onView: (xml: string) => void, onViewInstance?: (id: string) => void }) {
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);
  const [instances, setInstances] = useState<ProcessInstance[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [viewingId, setViewingId] = useState<string | null>(null);
  
  const [expandedVersions, setExpandedVersions] = useState<Set<string>>(new Set());
  const [expandedInstances, setExpandedInstances] = useState<Set<string>>(new Set());

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

  const toggleExpandedVersions = (bpmnId: string) => {
    setExpandedVersions(prev => {
      const next = new Set(prev);
      if (next.has(bpmnId)) next.delete(bpmnId);
      else next.add(bpmnId);
      return next;
    });
  };

  const toggleExpandedInstances = (bpmnId: string) => {
    setExpandedInstances(prev => {
      const next = new Set(prev);
      if (next.has(bpmnId)) next.delete(bpmnId);
      else next.add(bpmnId);
      return next;
    });
  };

  const handleDownload = async (defId: string) => {
    setDownloading(defId);
    try {
      const xml = await getDefinitionXml(defId);
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
    } catch (e: any) {
      alert('Failed to load definition: ' + e);
    } finally {
      setViewingId(null);
    }
  };

  const handleDelete = async (defId: string) => {
    const relatedInstances = instances.filter(i => i.definition_key === defId);
    let cascade = false;

    if (relatedInstances.length > 0) {
      const msg = `This version has ${relatedInstances.length} associated instance(s). Deleting it will also permanently delete all associated instances.\n\nAre you sure?`;
      if (!window.confirm(msg)) return;
      cascade = true;
    } else {
      if (!window.confirm("Delete this version?")) return;
    }

    try {
      await deleteDefinition(defId, cascade);
      fetchDefinitions();
    } catch (e: any) {
      alert('Delete failed: ' + e);
    }
  };

  const grouped = groupByProcess(definitions);

  return (
    <div>
      <h2>Deployed Processes</h2>
      <div className="header-actions">
        <button className="button" onClick={fetchDefinitions} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><RefreshCw size={16} /> Refresh</button>
      </div>

      {loading && <div style={{ margin: 20 }}>Loading definitions...</div>}
      {error && <div style={{ margin: 20, color: '#dc2626' }}>Error: {error}</div>}
      {!loading && !error && grouped.size === 0 && (
        <div style={{ margin: 20 }}>No deployed processes.</div>
      )}

      {[...grouped.entries()].map(([bpmnId, versions]) => {
        const latest = versions[0];
        const olderVersions = versions.slice(1);
        const instancesForProcess = instances.filter(i => versions.some(v => v.key === i.definition_key) && i.state !== 'Completed');
        const showVersions = expandedVersions.has(bpmnId);
        const showInstances = expandedInstances.has(bpmnId);

        return (
          <div key={bpmnId} className="process-group-card">
            
            <div className="process-group-header">
              <div className="process-title">
                <FileCode2 size={24} color="#2563eb" /> {bpmnId}
              </div>
              <div className="process-stats">
                <span className="stat-pill">{versions.length} deployed version{versions.length > 1 ? 's' : ''}</span>
                {instancesForProcess.length > 0 && (
                  <span className="stat-pill highlight">{instancesForProcess.length} active instance{instancesForProcess.length > 1 ? 's' : ''}</span>
                )}
              </div>
            </div>

            <div className="process-group-body">
              
              {/* LATEST VERSION BOX */}
              <div className="latest-version-box">
                <div className="latest-version-info">
                  <div style={{ display: 'flex', alignItems: 'center', gap: '10px', marginBottom: '4px' }}>
                    <span className="version-pill latest">v{latest.version} (Latest)</span>
                  </div>
                  <div style={{ display: 'flex', gap: '16px' }}>
                    <span className="meta-item"><Network size={16}/> {latest.node_count} process nodes</span>
                    <span className="meta-item"><Key size={16}/> Key: {latest.key.substring(0, 16)}…</span>
                  </div>
                </div>
                
                <div className="def-card-actions" style={{ margin: 0 }}>
                  <button className="button" onClick={() => handleView(latest.key)} disabled={viewingId === latest.key} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                    <Eye size={16} /> {viewingId === latest.key ? 'Loading...' : 'View BPMN'}
                  </button>
                  <button className="button button-secondary" onClick={() => handleDownload(latest.key)} disabled={downloading === latest.key} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                    <Download size={16} /> Download
                  </button>
                  <button className="button button-secondary" onClick={() => handleDelete(latest.key)} style={{ display: 'flex', alignItems: 'center', gap: '6px', color: '#ef4444', borderColor: '#ef4444' }} title="Delete latest version">
                    <Trash size={16} />
                  </button>
                </div>
              </div>

              {/* OLDER VERSIONS ACCORDION */}
              {olderVersions.length > 0 && (
                <div style={{ marginTop: '16px' }}>
                  <button className="accordion-toggle" onClick={() => toggleExpandedVersions(bpmnId)} style={{ borderRadius: showVersions ? '6px 6px 0 0' : '6px' }}>
                    <span style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <Boxes size={18} color="#64748b" /> Older Versions ({olderVersions.length})
                    </span>
                    {showVersions ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                  </button>
                  
                  {showVersions && (
                    <div className="older-versions-list">
                      {olderVersions.map(ver => {
                        const verInstances = instances.filter(i => i.definition_key === ver.key && i.state !== 'Completed');
                        return (
                          <div key={ver.key} className="older-version-row">
                            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                              <span className="version-pill older">v{ver.version}</span>
                              <span className="meta-item"><Network size={14}/> {ver.node_count} nodes</span>
                              <span className="meta-item"><Key size={14}/> {ver.key.substring(0, 8)}…</span>
                              {verInstances.length > 0 && (
                                <span className="stat-pill highlight">{verInstances.length} active</span>
                              )}
                            </div>
                            <div style={{ display: 'flex', gap: '8px' }}>
                              <button className="button" onClick={() => handleView(ver.key)} style={{ padding: '6px 12px' }}>
                                <Eye size={14} />
                              </button>
                              <button className="button button-secondary" onClick={() => handleDownload(ver.key)} style={{ padding: '6px 12px' }}>
                                <Download size={14} />
                              </button>
                              <button className="button button-secondary" onClick={() => handleDelete(ver.key)} style={{ padding: '6px 12px', color: '#ef4444', borderColor: '#ef4444' }}>
                                <Trash size={14} />
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}

              {/* RUNNING INSTANCES ACCORDION */}
              {instancesForProcess.length > 0 && (
                <div style={{ marginTop: '16px' }}>
                  <button className="accordion-toggle" onClick={() => toggleExpandedInstances(bpmnId)} style={{ borderRadius: showInstances ? '6px 6px 0 0' : '6px' }}>
                    <span style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <Activity size={18} color="#10b981" /> Active Instances ({instancesForProcess.length})
                    </span>
                    {showInstances ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                  </button>

                  {showInstances && (
                    <div style={{ border: '1px solid #e2e8f0', borderTop: 'none', padding: '16px', background: '#f8fafc', borderRadius: '0 0 6px 6px' }}>
                      {instancesForProcess.map(inst => {
                        const instDef = versions.find(v => v.key === inst.definition_key);
                        return (
                          <div key={inst.id} className="instance-list-item" onClick={() => onViewInstance?.(inst.id)}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                              {inst.state === 'Running' ? <Activity size={16} color="#10b981" /> : <Clock size={16} color="#f59e0b" />}
                              <span style={{ fontWeight: 600, color: '#1e293b' }}>{inst.business_key || inst.id.substring(0, 8)}</span>
                            </div>
                            <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
                               {instDef && (
                                <span className={`version-pill ${instDef.is_latest ? 'latest' : 'older'}`}>
                                  v{instDef.version}
                                </span>
                              )}
                              <span className="stat-pill">Current: {inst.current_node}</span>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}

            </div>
          </div>
        );
      })}
    </div>
  );
}

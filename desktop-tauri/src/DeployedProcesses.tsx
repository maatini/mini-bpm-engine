import { useState, useEffect, useCallback } from 'react';
import { save } from '@tauri-apps/api/dialog';
import { writeTextFile } from '@tauri-apps/api/fs';
import { listDefinitions, getDefinitionXml, type DefinitionInfo } from './lib/tauri';

export function DeployedProcesses({ onView }: { onView: (xml: string) => void }) {
  const [definitions, setDefinitions] = useState<DefinitionInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [viewingId, setViewingId] = useState<string | null>(null);

  const fetchDefinitions = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await listDefinitions();
      setDefinitions(list);
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

  return (
    <div>
      <h2>Deployed Processes</h2>
      <div className="header-actions">
        <button className="button" onClick={fetchDefinitions}>Refresh</button>
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
            >
              {viewingId === def.key ? 'Loading...' : 'View in Modeler'}
            </button>
            <button
              className="button button-secondary"
              onClick={() => handleDownload(def.key)}
              disabled={downloading === def.key}
            >
              {downloading === def.key ? 'Downloading...' : 'Download BPMN'}
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

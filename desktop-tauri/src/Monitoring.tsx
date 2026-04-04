import { useState, useEffect, useRef } from 'react'
import { getMonitoringData, type MonitoringData } from './lib/tauri'

/**
 * Formats bytes into a human-readable string (B, KB, MB, GB).
 */
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / Math.pow(1024, i)
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

/** Maps bucket_type to a human-readable label. */
function bucketTypeLabel(t: string): string {
  if (t === 'kv') return 'KV Store'
  if (t === 'object_store') return 'Object Store'
  if (t === 'stream') return 'Stream'
  return t
}

export function Monitoring() {
  const [data, setData] = useState<MonitoringData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const refresh = async () => {
    try {
      const result = await getMonitoringData()
      setData(result)
      setError(null)
    } catch (e: any) {
      setError(String(e))
    }
  }

  useEffect(() => {
    refresh()
    intervalRef.current = setInterval(refresh, 5000)
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [])

  return (
    <div>
      <h2>Monitoring</h2>

      {error && (
        <div className="card" style={{ color: '#991b1b' }}>
          Error loading monitoring data: {error}
        </div>
      )}

      {/* Engine Metrics */}
      <div className="card">
        <div className="card-title">🔧 Engine Metrics</div>
        <div className="metric-grid">
          <div className="metric-card">
            <div className="metric-value">{data?.definitions_count ?? '–'}</div>
            <div className="metric-label">Deployed Definitions</div>
          </div>
          <div className="metric-card">
            <div className="metric-value">{data?.instances_total ?? '–'}</div>
            <div className="metric-label">Total Instances</div>
          </div>
          <div className="metric-card metric-card-running">
            <div className="metric-value">{data?.instances_running ?? '–'}</div>
            <div className="metric-label">Running</div>
          </div>
          <div className="metric-card metric-card-completed">
            <div className="metric-value">{data?.instances_completed ?? '–'}</div>
            <div className="metric-label">Completed</div>
          </div>
          <div className="metric-card">
            <div className="metric-value">{data?.pending_user_tasks ?? '–'}</div>
            <div className="metric-label">Pending User Tasks</div>
          </div>
          <div className="metric-card">
            <div className="metric-value">{data?.pending_service_tasks ?? '–'}</div>
            <div className="metric-label">Pending External Tasks</div>
          </div>
          <div className="metric-card">
            <div className="metric-value">{data?.pending_timers ?? '–'}</div>
            <div className="metric-label">Pending Timers</div>
          </div>
          <div className="metric-card">
            <div className="metric-value">{data?.pending_message_catches ?? '–'}</div>
            <div className="metric-label">Pending Messages</div>
          </div>
        </div>
      </div>

      {/* Storage Backend Info */}
      <div className="card">
        <div className="card-title">📡 Storage Backend</div>
        {data?.storage_info ? (
          <div className="metric-grid">
            <div className="metric-card">
              <div className="metric-value" style={{ fontSize: '1rem' }}>
                {data.storage_info.backend_name}
              </div>
              <div className="metric-label">Backend</div>
            </div>
            <div className="metric-card">
              <div className="metric-value" style={{ fontSize: '1rem' }}>
                v{data.storage_info.version}
              </div>
              <div className="metric-label">Version</div>
            </div>
            <div className="metric-card">
              <div className="metric-value" style={{ fontSize: '1rem' }}>
                {data.storage_info.host}:{data.storage_info.port}
              </div>
              <div className="metric-label">Endpoint</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">{data.storage_info.streams}</div>
              <div className="metric-label">Streams</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">{data.storage_info.consumers}</div>
              <div className="metric-label">Consumers</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">{formatBytes(data.storage_info.memory_bytes)}</div>
              <div className="metric-label">Memory Usage</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">{formatBytes(data.storage_info.storage_bytes)}</div>
              <div className="metric-label">Storage Usage</div>
            </div>
          </div>
        ) : (
          <div style={{ color: '#64748b', fontStyle: 'italic' }}>
            No storage backend connected — running in-memory only.
          </div>
        )}
      </div>

      {/* Data Storage Details */}
      {data?.storage_info && data.storage_info.buckets.length > 0 && (
        <div className="card">
          <div className="card-title">📦 Data Storage Details</div>
          <table className="variables-table" style={{ width: '100%' }}>
            <thead>
              <tr>
                <th style={{ textAlign: 'left' }}>Bucket</th>
                <th style={{ textAlign: 'left' }}>Type</th>
                <th style={{ textAlign: 'right' }}>Entries</th>
                <th style={{ textAlign: 'right' }}>Size</th>
              </tr>
            </thead>
            <tbody>
              {data.storage_info.buckets.map((b) => (
                <tr key={b.name}>
                  <td style={{ fontWeight: 500 }}>{b.name}</td>
                  <td>
                    <span style={{
                      fontSize: '0.75rem',
                      fontWeight: 600,
                      padding: '2px 8px',
                      borderRadius: '4px',
                      background: b.bucket_type === 'kv' ? '#dbeafe' :
                                  b.bucket_type === 'object_store' ? '#fef3c7' : '#d1fae5',
                      color: b.bucket_type === 'kv' ? '#1e40af' :
                             b.bucket_type === 'object_store' ? '#92400e' : '#065f46',
                    }}>
                      {bucketTypeLabel(b.bucket_type)}
                    </span>
                  </td>
                  <td style={{ textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>
                    {b.entries.toLocaleString()}
                  </td>
                  <td style={{ textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>
                    {formatBytes(b.size_bytes)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div style={{ padding: '0 20px', fontSize: '0.8rem', color: '#94a3b8' }}>
        Auto-refreshing every 5 s
      </div>
    </div>
  )
}

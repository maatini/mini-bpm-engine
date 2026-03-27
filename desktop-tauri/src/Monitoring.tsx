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

export function Monitoring() {
  const [data, setData] = useState<MonitoringData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const refresh = async () => {
    try {
      const result = await getMonitoringData()
      setData(result)
      setError(null)
    } catch (e) {
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
        </div>
      </div>

      {/* NATS Server Info */}
      <div className="card">
        <div className="card-title">📡 NATS Server</div>
        {data?.nats_server ? (
          <div className="metric-grid">
            <div className="metric-card">
              <div className="metric-value" style={{ fontSize: '1rem' }}>
                {data.nats_server.server_name}
              </div>
              <div className="metric-label">Server Name</div>
            </div>
            <div className="metric-card">
              <div className="metric-value" style={{ fontSize: '1rem' }}>
                v{data.nats_server.version}
              </div>
              <div className="metric-label">Version</div>
            </div>
            <div className="metric-card">
              <div className="metric-value" style={{ fontSize: '1rem' }}>
                {data.nats_server.host}:{data.nats_server.port}
              </div>
              <div className="metric-label">Endpoint</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">{data.nats_server.streams}</div>
              <div className="metric-label">JetStream Streams</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">{data.nats_server.consumers}</div>
              <div className="metric-label">JetStream Consumers</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">
                {formatBytes(data.nats_server.memory_bytes)}
              </div>
              <div className="metric-label">Memory Usage</div>
            </div>
            <div className="metric-card">
              <div className="metric-value">
                {formatBytes(data.nats_server.storage_bytes)}
              </div>
              <div className="metric-label">Storage Usage</div>
            </div>
          </div>
        ) : (
          <div style={{ color: '#64748b', fontStyle: 'italic' }}>
            NATS not connected — switch to NATS in Settings to see server metrics.
          </div>
        )}
      </div>

      <div style={{ padding: '0 20px', fontSize: '0.8rem', color: '#94a3b8' }}>
        Auto-refreshing every 5 s
      </div>
    </div>
  )
}

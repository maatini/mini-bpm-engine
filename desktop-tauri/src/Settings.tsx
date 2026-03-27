import { useState, useEffect } from 'react'
import { getBackendInfo, switchBackend, type BackendInfo } from './lib/tauri'
import { Server, Cpu } from 'lucide-react'

interface SettingsProps {
  onBackendChange?: (info: BackendInfo) => void
}

export function Settings({ onBackendChange }: SettingsProps) {
  const [info, setInfo] = useState<BackendInfo | null>(null)
  const [natsUrl, setNatsUrl] = useState('nats://localhost:4222')
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)

  useEffect(() => {
    loadInfo()
  }, [])

  const loadInfo = async () => {
    try {
      const result = await getBackendInfo()
      setInfo(result)
      if (result.nats_url) {
        setNatsUrl(result.nats_url)
      }
    } catch (e) {
      console.error('Failed to load backend info', e)
    }
  }

  const handleSwitch = async (backendType: string) => {
    setLoading(true)
    setMessage(null)
    try {
      const result = await switchBackend(backendType, backendType === 'nats' ? natsUrl : undefined)
      setInfo(result)
      setMessage({ type: 'success', text: `Switched to ${result.backend_type} backend.` })
      onBackendChange?.(result)
    } catch (e) {
      setMessage({ type: 'error', text: String(e) })
    } finally {
      setLoading(false)
    }
  }

  return (
    <div>
      <h2>Settings</h2>
      <div className="card" style={{ maxWidth: 520 }}>
        <div className="card-title">Backend Configuration</div>

        {/* Current status */}
        <div style={{ marginBottom: 16 }}>
          <span style={{ marginRight: 8 }}>Active Backend:</span>
          {info && (
            <span className={`backend-badge ${info.backend_type === 'nats' ? 'backend-nats' : 'backend-inmemory'}`}>
              {info.backend_type === 'nats' ? '● NATS' : '● In-Memory'}
            </span>
          )}
        </div>

        {/* NATS URL input */}
        <div style={{ marginBottom: 12 }}>
          <label style={{ display: 'block', marginBottom: 4, fontWeight: 500, fontSize: '0.9rem' }}>
            NATS Server URL
          </label>
          <input
            className="input-field"
            type="text"
            value={natsUrl}
            onChange={(e) => setNatsUrl(e.target.value)}
            placeholder="nats://localhost:4222"
            disabled={loading}
          />
        </div>

        {/* Action buttons */}
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            className="button"
            onClick={() => handleSwitch('nats')}
            disabled={loading || info?.backend_type === 'nats'}
            style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <Server size={16} /> Connect to NATS
          </button>
          <button
            className="button-secondary"
            onClick={() => handleSwitch('in-memory')}
            disabled={loading || info?.backend_type === 'in-memory'}
            style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <Cpu size={16} /> Switch to In-Memory
          </button>
        </div>

        {/* Feedback message */}
        {message && (
          <div className={`status-message ${message.type === 'success' ? 'status-success' : 'status-error'}`}>
            {message.text}
          </div>
        )}
      </div>
    </div>
  )
}

import { useState, useEffect } from 'react'
import { getApiUrl, setApiUrl, getMonitoringData } from './lib/tauri'
import { Server, CheckCircle, XCircle } from 'lucide-react'

export function Settings() {
  const [apiUrl, setLocalApiUrl] = useState('http://localhost:8081')
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState<'idle' | 'success' | 'error'>('idle')
  const [message, setMessage] = useState<string | null>(null)

  useEffect(() => {
    loadInfo()
  }, [])

  const loadInfo = async () => {
    try {
      const currentUrl = await getApiUrl()
      setLocalApiUrl(currentUrl)
    } catch (e) {
      console.error('Failed to load API URL', e)
    }
  }

  const handleSaveAndVerify = async () => {
    setLoading(true)
    setStatus('idle')
    setMessage(null)
    
    try {
      // 1. Save internally in Tauri
      await setApiUrl(apiUrl)
      
      // 2. Verify connection by fetching monitoring data
      await getMonitoringData()
      
      setStatus('success')
      setMessage(`Successfully connected to Workflow Engine at ${apiUrl}`)
    } catch (e) {
      setStatus('error')
      setMessage(`Connection failed: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div>
      <h2>Settings</h2>
      
      <div className="card" style={{ maxWidth: 520 }}>
        <div className="card-title">Engine API Configuration</div>
        <p style={{ fontSize: '0.9rem', color: '#64748b', marginBottom: 20 }}>
          This desktop application operates as a Thin Client. It delegates workflow execution to the configured Engine Server via REST.
        </p>

        {/* API URL input */}
        <div style={{ marginBottom: 16 }}>
          <label style={{ display: 'block', marginBottom: 6, fontWeight: 500, fontSize: '0.9rem' }}>
            Engine REST API URL
          </label>
          <input
            className="input-field"
            type="text"
            value={apiUrl}
            onChange={(e) => setLocalApiUrl(e.target.value)}
            placeholder="http://localhost:8081"
            disabled={loading}
          />
        </div>

        {/* Action buttons */}
        <div style={{ display: 'flex', gap: 12, alignItems: 'center', marginBottom: 16 }}>
          <button
            className="button"
            onClick={handleSaveAndVerify}
            disabled={loading || !apiUrl.trim()}
            style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
          >
            <Server size={16} /> Save & Verify Connection
          </button>

          {status === 'success' && (
            <span style={{ color: '#16a34a', display: 'flex', alignItems: 'center', gap: '4px', fontSize: '0.9rem', fontWeight: 500 }}>
              <CheckCircle size={16} /> OK
            </span>
          )}
          {status === 'error' && (
            <span style={{ color: '#dc2626', display: 'flex', alignItems: 'center', gap: '4px', fontSize: '0.9rem', fontWeight: 500 }}>
              <XCircle size={16} /> Failed
            </span>
          )}
        </div>

        {/* Feedback message */}
        {message && (
          <div className={`status-message ${status === 'success' ? 'status-success' : 'status-error'}`}>
            {message}
          </div>
        )}
      </div>
    </div>
  )
}

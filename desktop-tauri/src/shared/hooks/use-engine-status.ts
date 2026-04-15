import { useState, useEffect, useCallback, useRef } from 'react'
import { getMonitoringData } from '../lib/tauri'

export type EngineStatus = 'checking' | 'online' | 'offline'

export function useEngineStatus(intervalMs = 10_000) {
  const [status, setStatus] = useState<EngineStatus>('checking')
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const check = useCallback(async () => {
    try {
      await getMonitoringData()
      setStatus('online')
    } catch {
      setStatus('offline')
    }
  }, [])

  useEffect(() => {
    check()
    intervalRef.current = setInterval(check, intervalMs)
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [check, intervalMs])

  return { status, refresh: check }
}

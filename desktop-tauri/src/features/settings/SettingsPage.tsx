import { useState, useEffect } from 'react'
import { getApiUrl, setApiUrl, getMonitoringData } from '../../shared/lib/tauri'
import { Server, CheckCircle, XCircle, Palette, Monitor, Sun, Moon, Wifi, WifiOff, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import type { EngineStatus } from '../../shared/hooks/use-engine-status'

interface Props {
  engineStatus: EngineStatus
  onConnectionChanged: () => void
}

export function SettingsPage({ engineStatus, onConnectionChanged }: Props) {
  const [apiUrl, setLocalApiUrl] = useState('http://localhost:8081')
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState<'idle' | 'success' | 'error'>('idle')
  const [message, setMessage] = useState<string | null>(null)

  const [theme, setTheme] = useState<'auto' | 'light' | 'dark'>(
    () => (localStorage.getItem('theme') as any) || 'auto'
  )

  useEffect(() => {
    loadInfo()
  }, [])

  const loadInfo = async () => {
    try {
      const currentUrl = await getApiUrl()
      setLocalApiUrl(currentUrl)
    } catch (e: any) {
      console.error('Failed to load API URL', e)
    }
  }

  const handleSaveAndVerify = async () => {
    setLoading(true)
    setStatus('idle')
    setMessage(null)

    try {
      // 1. URL in Tauri speichern
      await setApiUrl(apiUrl)

      // 2. Verbindung direkt prüfen
      await getMonitoringData()

      setStatus('success')
      setMessage(`Verbindung zu Engine erfolgreich: ${apiUrl}`)
    } catch (e: any) {
      setStatus('error')
      setMessage(`Verbindung fehlgeschlagen: ${String(e)}`)
    } finally {
      setLoading(false)
      // Globalen Status in App aktualisieren
      onConnectionChanged()
    }
  }

  const handleThemeChange = (newTheme: 'auto' | 'light' | 'dark') => {
    setTheme(newTheme)
    localStorage.setItem('theme', newTheme)
    if (newTheme === 'auto') {
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      document.documentElement.setAttribute('data-theme', prefersDark ? 'dark' : 'light');
    } else {
      document.documentElement.setAttribute('data-theme', newTheme)
    }
  }

  return (
    <div className="flex flex-col h-full bg-background">
      <div className="flex items-center justify-between px-6 py-4 border-b bg-background">
        <h2 className="text-2xl font-bold tracking-tight">Settings</h2>
      </div>
      
      <div className="p-6 space-y-6 max-w-2xl">
        <Card>
          <CardHeader>
            <div className="flex items-start justify-between gap-4">
              <div>
                <CardTitle>Engine API Configuration</CardTitle>
                <CardDescription>
                  This desktop application operates as a Thin Client. It delegates workflow execution to the configured Engine Server via REST.
                </CardDescription>
              </div>
              {/* Live-Status-Badge */}
              <div className="flex-shrink-0 flex items-center gap-1.5 text-sm font-medium">
                {engineStatus === 'checking' && (
                  <><Loader2 className="h-4 w-4 animate-spin text-muted-foreground" /><span className="text-muted-foreground">Verbinde...</span></>
                )}
                {engineStatus === 'online' && (
                  <><Wifi className="h-4 w-4 text-green-500" /><span className="text-green-600 dark:text-green-500">Online</span></>
                )}
                {engineStatus === 'offline' && (
                  <><WifiOff className="h-4 w-4 text-destructive" /><span className="text-destructive">Offline</span></>
                )}
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {/* API URL input */}
            <div className="space-y-2">
              <Label htmlFor="apiUrl">Engine REST API URL</Label>
              <Input
                id="apiUrl"
                type="text"
                value={apiUrl}
                onChange={(e: any) => setLocalApiUrl(e.target.value)}
                placeholder="http://localhost:8081"
                disabled={loading}
              />
            </div>

            {/* Action buttons */}
            <div className="flex items-center gap-4 pt-2">
              <Button
                onClick={handleSaveAndVerify}
                disabled={loading || !apiUrl.trim()}
                className="gap-2"
              >
                <Server className="h-4 w-4" /> Save & Verify Connection
              </Button>

              {status === 'success' && (
                <span className="flex items-center gap-1.5 text-sm font-medium text-green-600 dark:text-green-500">
                  <CheckCircle className="h-4 w-4" /> OK
                </span>
              )}
              {status === 'error' && (
                <span className="flex items-center gap-1.5 text-sm font-medium text-destructive">
                  <XCircle className="h-4 w-4" /> Failed
                </span>
              )}
            </div>

            {/* Feedback message */}
            {message && (
              <div className={`p-3 rounded-md text-sm border mt-4 ${
                status === 'success' 
                  ? 'bg-green-50 text-green-700 border-green-200 dark:bg-green-500/10 dark:text-green-400 dark:border-green-500/20' 
                  : 'bg-destructive/10 text-destructive border-destructive/20'
              }`}>
                {message}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2"><Palette className="h-5 w-5" /> Appearance</CardTitle>
            <CardDescription>
              Choose your preferred theme for the user interface.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              <Button 
                variant={theme === 'auto' ? 'default' : 'outline'}
                onClick={() => handleThemeChange('auto')}
              >
                <Monitor className="h-4 w-4 mr-1.5" /> System
              </Button>
              <Button 
                variant={theme === 'light' ? 'default' : 'outline'}
                onClick={() => handleThemeChange('light')}
              >
                <Sun className="h-4 w-4 mr-1.5" /> Light
              </Button>
              <Button 
                variant={theme === 'dark' ? 'default' : 'outline'}
                onClick={() => handleThemeChange('dark')}
              >
                <Moon className="h-4 w-4 mr-1.5" /> Dark
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

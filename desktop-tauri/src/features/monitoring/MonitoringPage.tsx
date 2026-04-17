import { useState, useEffect, useRef } from 'react'
import { useEngineEvents } from '../../shared/hooks/use-engine-events'
import { getMonitoringData, getBucketEntries, getBucketEntryDetail, type MonitoringData, type BucketEntry, type BucketEntryDetail } from '../../shared/lib/tauri'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Table, TableHeader, TableRow, TableHead, TableBody, TableCell } from '@/components/ui/table'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Server, Settings2, Database, List, ExternalLink, Cloud, HardDrive } from 'lucide-react'
import { Skeleton } from '@/components/ui/skeleton'
import { LogStream } from './LogStream'
import { DataViewer } from '@/shared/components/DataViewer'

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

export function MonitoringPage() {
  const [data, setData] = useState<MonitoringData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const [selectedBucket, setSelectedBucket] = useState<string | null>(null)
  const [bucketEntries, setBucketEntries] = useState<BucketEntry[]>([])
  const [loadingEntries, setLoadingEntries] = useState(false)

  const [selectedEntryKey, setSelectedEntryKey] = useState<string | null>(null)
  const [entryDetail, setEntryDetail] = useState<BucketEntryDetail | null>(null)
  const [loadingDetail, setLoadingDetail] = useState(false)

  const handleBucketClick = async (bucket: string) => {
    setSelectedBucket(bucket)
    setBucketEntries([])
    setLoadingEntries(true)
    try {
      const entries = await getBucketEntries(bucket, 0, 200) // load up to 200 for now
      setBucketEntries(entries)
    } catch (e) {
      console.error(e)
    } finally {
      setLoadingEntries(false)
    }
  }

  const handleEntryClick = async (key: string) => {
    if (!selectedBucket) return
    setSelectedEntryKey(key)
    setEntryDetail(null)
    setLoadingDetail(true)
    try {
      const detail = await getBucketEntryDetail(selectedBucket, key)
      setEntryDetail(detail)
    } catch (e) {
      console.error(e)
    } finally {
      setLoadingDetail(false)
    }
  }

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
    intervalRef.current = setInterval(refresh, 30000)
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [])
  useEngineEvents(refresh);

  return (
    <div className="flex flex-col h-full bg-background">
      <div className="flex items-center justify-between px-6 py-4 border-b bg-background">
        <h2 className="text-2xl font-bold tracking-tight">Monitoring</h2>
        <div className="flex items-center gap-2">
          {data != null && (
            data.storage_info != null ? (
              <Badge className="gap-1 bg-emerald-100 text-emerald-700 hover:bg-emerald-100/80 dark:bg-emerald-900/40 dark:text-emerald-400 border-emerald-200 dark:border-emerald-800">
                <Cloud className="h-3 w-3" />
                NATS
              </Badge>
            ) : (
              <Badge className="gap-1 bg-amber-100 text-amber-700 hover:bg-amber-100/80 dark:bg-amber-900/40 dark:text-amber-400 border-amber-200 dark:border-amber-800">
                <HardDrive className="h-3 w-3" />
                In-Memory
              </Badge>
            )
          )}
          <span className="text-xs text-muted-foreground bg-muted px-2 py-1 rounded-md">Push-Updates aktiv</span>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-6">
          {error && (
            <Card className="border-destructive/50 bg-destructive/10">
              <CardContent className="p-4 text-destructive font-medium">
                Error loading monitoring data: {error}
              </CardContent>
            </Card>
          )}

          {/* Engine Metrics */}
          <Card>
            <CardHeader className="pb-3 border-b bg-muted/20">
              <CardTitle className="text-lg flex items-center gap-2">
                <Settings2 className="h-5 w-5 text-primary" /> Engine Metrics
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-6">
              {!data && !error && (
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  {[1,2,3,4,5,6,7,8].map(i => (
                    <Card key={i} className="bg-muted/30 border-muted-foreground/20">
                      <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                        <Skeleton className="h-8 w-12 mb-2" />
                        <Skeleton className="h-3 w-[100px]" />
                      </CardContent>
                    </Card>
                  ))}
                </div>
              )}
              {data && (
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <Card className="bg-muted/30 border-muted-foreground/20">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-3xl font-bold tracking-tight">{data?.definitions_count ?? '–'}</span>
                    <span className="text-xs text-muted-foreground mt-1 uppercase tracking-wider font-semibold">Deployed Definitions</span>
                  </CardContent>
                </Card>
                <Card className="bg-muted/30 border-muted-foreground/20">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-3xl font-bold tracking-tight">{data?.instances_total ?? '–'}</span>
                    <span className="text-xs text-muted-foreground mt-1 uppercase tracking-wider font-semibold">Total Instances</span>
                  </CardContent>
                </Card>
                <Card className="border-blue-500/30 bg-blue-500/5">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-3xl font-bold tracking-tight text-blue-600 dark:text-blue-400">{data?.instances_running ?? '–'}</span>
                    <span className="text-xs text-blue-600/80 dark:text-blue-400/80 mt-1 uppercase tracking-wider font-semibold">Running</span>
                  </CardContent>
                </Card>
                <Card className="border-green-500/30 bg-green-500/5">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-3xl font-bold tracking-tight text-green-600 dark:text-green-400">{data?.instances_completed ?? '–'}</span>
                    <span className="text-xs text-green-600/80 dark:text-green-400/80 mt-1 uppercase tracking-wider font-semibold">Completed</span>
                  </CardContent>
                </Card>
                <Card className="bg-muted/30 border-muted-foreground/20">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-2xl font-bold">{data?.pending_user_tasks ?? '–'}</span>
                    <span className="text-xs text-muted-foreground mt-1 text-center">Pending User Tasks</span>
                  </CardContent>
                </Card>
                <Card className="bg-muted/30 border-muted-foreground/20">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-2xl font-bold">{data?.pending_service_tasks ?? '–'}</span>
                    <span className="text-xs text-muted-foreground mt-1 text-center">Pending External Tasks</span>
                  </CardContent>
                </Card>
                <Card className="bg-muted/30 border-muted-foreground/20">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-2xl font-bold">{data?.pending_timers ?? '–'}</span>
                    <span className="text-xs text-muted-foreground mt-1 text-center">Pending Timers</span>
                  </CardContent>
                </Card>
                <Card className="bg-muted/30 border-muted-foreground/20">
                  <CardContent className="p-4 flex flex-col items-center justify-center text-center h-full">
                    <span className="text-2xl font-bold">{data?.pending_message_catches ?? '–'}</span>
                    <span className="text-xs text-muted-foreground mt-1 text-center">Pending Messages</span>
                  </CardContent>
                </Card>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Storage Backend Info */}
          <Card>
            <CardHeader className="pb-3 border-b bg-muted/20">
              <CardTitle className="text-lg flex items-center gap-2">
                <Server className="h-5 w-5 text-emerald-500" /> Storage Backend
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-6">
              {data?.storage_info ? (
                <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
                  <div className="bg-muted/30 border rounded-lg p-3 text-center min-w-0">
                    <div className="text-lg font-bold truncate px-1" title={data.storage_info.backend_name}>{data.storage_info.backend_name}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Backend</div>
                  </div>
                  <div className="bg-muted/30 border rounded-lg p-3 text-center">
                    <div className="text-lg font-bold">v{data.storage_info.version}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Version</div>
                  </div>
                  <div className="bg-muted/30 border rounded-lg p-3 text-center min-w-0">
                    <div className="text-sm font-semibold truncate mt-1 px-1" title={`${data.storage_info.host}:${data.storage_info.port}`}>{data.storage_info.host}:{data.storage_info.port}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Endpoint</div>
                  </div>
                  <div className="bg-muted/30 border rounded-lg p-3 text-center">
                    <div className="text-lg font-bold">{data.storage_info.streams}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Streams</div>
                  </div>
                  <div className="bg-muted/30 border rounded-lg p-3 text-center">
                    <div className="text-lg font-bold">{data.storage_info.consumers}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Consumers</div>
                  </div>
                  <div className="bg-muted/30 border rounded-lg p-3 text-center">
                    <div className="text-lg font-bold">{formatBytes(data.storage_info.memory_bytes)}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Memory Usage</div>
                  </div>
                  <div className="bg-muted/30 border rounded-lg p-3 text-center">
                    <div className="text-lg font-bold">{formatBytes(data.storage_info.storage_bytes)}</div>
                    <div className="text-xs text-muted-foreground mt-0.5">Storage Usage</div>
                  </div>
                </div>
              ) : (
                <div className="text-muted-foreground italic py-4 text-center border rounded-lg bg-muted/10">
                  No storage backend connected — running in-memory only.
                </div>
              )}
            </CardContent>
          </Card>

          {/* Data Storage Details */}
          {data?.storage_info && data.storage_info.buckets.length > 0 && (
            <Card>
              <CardHeader className="pb-3 border-b bg-muted/20">
                <CardTitle className="text-lg flex items-center gap-2">
                  <Database className="h-5 w-5 text-amber-500" /> Data Storage Details
                </CardTitle>
              </CardHeader>
              <CardContent className="p-0">
                <Table>
                  <TableHeader className="bg-muted/30">
                    <TableRow>
                      <TableHead>Bucket</TableHead>
                      <TableHead>Type</TableHead>
                      <TableHead className="text-right">Entries</TableHead>
                      <TableHead className="text-right">Size</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.storage_info.buckets.map((b) => (
                      <TableRow key={b.name} className="hover:bg-muted/50 cursor-pointer" onClick={() => handleBucketClick(b.name)}>
                        <TableCell className="font-medium text-foreground">{b.name}</TableCell>
                        <TableCell>
                          <Badge 
                            variant="secondary" 
                            className={
                              b.bucket_type === 'kv' ? 'bg-blue-100 text-blue-700 hover:bg-blue-100/80 dark:bg-blue-900/40 dark:text-blue-400' :
                              b.bucket_type === 'object_store' ? 'bg-amber-100 text-amber-700 hover:bg-amber-100/80 dark:bg-amber-900/40 dark:text-amber-400' : 
                              'bg-green-100 text-green-700 hover:bg-green-100/80 dark:bg-green-900/40 dark:text-green-400'
                            }
                          >
                            {bucketTypeLabel(b.bucket_type)}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-right tabular-nums">{b.entries.toLocaleString()}</TableCell>
                        <TableCell className="text-right tabular-nums text-muted-foreground">{formatBytes(b.size_bytes)}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          )}

          {/* Log Stream */}
          <Card className="overflow-hidden">
            <CardHeader className="pb-3 border-b bg-muted/20 py-4">
              <CardTitle className="text-lg flex items-center gap-2">
                Log Stream
              </CardTitle>
            </CardHeader>
            <CardContent className="p-0 h-[420px] flex flex-col">
              <LogStream />
            </CardContent>
          </Card>

        </div>
      </ScrollArea>

      {/* Bucket Entries Dialog */}
      <Dialog open={!!selectedBucket && !selectedEntryKey} onOpenChange={(open) => !open && setSelectedBucket(null)}>
        <DialogContent className="max-w-4xl max-h-[80vh] flex flex-col p-0">
          <DialogHeader className="p-6 pb-2 border-b">
            <DialogTitle className="flex items-center gap-2">
              <List className="h-5 w-5" /> Bucket Entries: {selectedBucket}
            </DialogTitle>
            <DialogDescription className="sr-only">Einträge im Bucket {selectedBucket}</DialogDescription>
          </DialogHeader>
          <div className="flex-1 overflow-auto p-0">
            {loadingEntries ? (
              <div className="p-6 space-y-4">
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
              </div>
            ) : bucketEntries.length === 0 ? (
              <div className="p-10 text-center text-muted-foreground">No entries found.</div>
            ) : (
              <Table>
                <TableHeader className="bg-muted/30 sticky top-0 backdrop-blur-sm z-10">
                  <TableRow>
                    <TableHead>Key</TableHead>
                    <TableHead>Created At</TableHead>
                    <TableHead className="text-right">Size (Bytes)</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {bucketEntries.map((entry) => (
                    <TableRow key={entry.key} className="cursor-pointer hover:bg-muted/50" onClick={() => handleEntryClick(entry.key)}>
                      <TableCell className="font-medium font-mono text-xs">{entry.key}</TableCell>
                      <TableCell className="text-muted-foreground tabular-nums text-xs">
                        {entry.created_at ? new Date(entry.created_at).toLocaleString() : '—'}
                      </TableCell>
                      <TableCell className="text-right tabular-nums text-muted-foreground">
                        {entry.size_bytes !== null ? formatBytes(entry.size_bytes) : '—'}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* Entry Detail Dialog */}
      <Dialog open={!!selectedEntryKey} onOpenChange={(open) => {
        if (!open) {
          setSelectedEntryKey(null)
          setEntryDetail(null)
        }
      }}>
        <DialogContent className="max-w-5xl max-h-[85vh] flex flex-col p-0">
          <DialogHeader className="p-6 pb-2 border-b">
            <DialogTitle className="flex items-center gap-2">
              <ExternalLink className="h-5 w-5" /> Detail: {selectedEntryKey}
            </DialogTitle>
            <DialogDescription className="sr-only">Details für Eintrag {selectedEntryKey}</DialogDescription>
          </DialogHeader>
          <div className="flex-1 overflow-auto bg-muted/10 p-6">
            {loadingDetail ? (
              <div className="space-y-4">
                <Skeleton className="h-8 w-1/3" />
                <Skeleton className="h-[200px] w-full" />
              </div>
            ) : entryDetail ? (
              <DataViewer
                content={entryDetail.data}
                filename={entryDetail.key}
                encoding={entryDetail.encoding}
                height="500px"
              />
            ) : null}
          </div>
        </DialogContent>
      </Dialog>

    </div>
  )
}

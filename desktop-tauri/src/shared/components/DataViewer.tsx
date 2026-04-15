import Editor from '@monaco-editor/react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import xmlFormatter from 'xml-formatter'
import { Copy, Check } from 'lucide-react'
import { useState, useMemo } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

export type DataFormat = 'auto' | 'json' | 'xml' | 'xaml' | 'yaml' | 'bpmn' | 'csv' | 'markdown'

interface DataViewerProps {
  content: string
  filename?: string
  format?: DataFormat
  height?: string
  className?: string
}

function detectFormat(content: string, filename?: string): Exclude<DataFormat, 'auto'> {
  if (filename) {
    const ext = filename.split('.').pop()?.toLowerCase()
    if (ext === 'json') return 'json'
    if (ext === 'xml') return 'xml'
    if (ext === 'xaml') return 'xaml'
    if (ext === 'bpmn') return 'bpmn'
    if (ext === 'yaml' || ext === 'yml') return 'yaml'
    if (ext === 'csv') return 'csv'
    if (ext === 'md' || ext === 'markdown') return 'markdown'
  }

  const trimmed = content.trimStart()

  try {
    JSON.parse(content)
    return 'json'
  } catch { /* weiter */ }

  if (trimmed.startsWith('<')) {
    if (trimmed.includes('xmlns:bpmn') || trimmed.includes('bpmn:definitions') || trimmed.includes('bpmn2:definitions')) return 'bpmn'
    return 'xml'
  }

  const firstLine = trimmed.split('\n')[0]
  if (/^---/.test(trimmed) || /^[a-zA-Z_][a-zA-Z0-9_]*\s*:/.test(firstLine)) return 'yaml'

  if ((firstLine.match(/,/g) ?? []).length > 1 && !firstLine.includes('<')) return 'csv'

  if (/^#{1,6} /m.test(content)) return 'markdown'

  return 'json'
}

function formatContent(content: string, format: Exclude<DataFormat, 'auto'>): string {
  try {
    if (format === 'json') {
      return JSON.stringify(JSON.parse(content), null, 2)
    }
    if (format === 'xml' || format === 'xaml' || format === 'bpmn') {
      return xmlFormatter(content, {
        indentation: '  ',
        collapseContent: true,
        lineSeparator: '\n',
      })
    }
  } catch { /* Fallback: Rohinhalt */ }
  return content
}

const FORMAT_LABELS: Record<Exclude<DataFormat, 'auto'>, string> = {
  json: 'JSON',
  xml: 'XML',
  xaml: 'XAML',
  yaml: 'YAML',
  bpmn: 'BPMN',
  csv: 'CSV',
  markdown: 'Markdown',
}

const MONACO_LANG: Record<Exclude<DataFormat, 'auto'>, string> = {
  json: 'json',
  xml: 'xml',
  xaml: 'xml',
  yaml: 'yaml',
  bpmn: 'xml',
  csv: 'plaintext',
  markdown: 'markdown',
}

export function DataViewer({ content, filename, format = 'auto', height = '400px', className }: DataViewerProps) {
  const [copied, setCopied] = useState(false)

  const resolved = format === 'auto' ? detectFormat(content, filename) : format
  const formatted = useMemo(() => formatContent(content, resolved), [content, resolved])

  const handleCopy = async () => {
    await navigator.clipboard.writeText(formatted)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className={cn('flex flex-col rounded-md border overflow-hidden', className)}>
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900 border-b border-zinc-700">
        <Badge variant="secondary" className="text-xs font-mono">
          {FORMAT_LABELS[resolved]}
        </Badge>
        <Button
          size="sm"
          variant="ghost"
          className="h-6 px-2 text-zinc-400 hover:text-zinc-100"
          onClick={handleCopy}
        >
          {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
          <span className="ml-1 text-xs">{copied ? 'Kopiert' : 'Kopieren'}</span>
        </Button>
      </div>

      {/* Inhalt */}
      {resolved === 'markdown' ? (
        <div
          className="flex-1 overflow-auto p-4 bg-background prose prose-sm dark:prose-invert max-w-none"
          style={{ minHeight: height }}
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
        </div>
      ) : (
        <Editor
          height={height}
          language={MONACO_LANG[resolved]}
          value={formatted}
          theme="vs-dark"
          options={{
            readOnly: true,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            fontSize: 13,
            lineNumbers: 'on',
            wordWrap: 'on',
            folding: true,
            renderLineHighlight: 'none',
            overviewRulerLanes: 0,
            hideCursorInOverviewRuler: true,
            scrollbar: { verticalScrollbarSize: 6, horizontalScrollbarSize: 6 },
          }}
        />
      )}
    </div>
  )
}

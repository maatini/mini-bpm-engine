import { useState, useEffect } from 'react'
import { deployDefinition, startInstance, startTimerInstance } from './shared/lib/tauri'
import { ModelerPage } from './features/modeler/ModelerPage'
import { InstancesPage } from './features/instances/InstancesPage'
import { DeployedProcessesPage } from './features/definitions/DeployedProcessesPage'
import { SettingsPage } from './features/settings/SettingsPage'
import { MonitoringPage } from './features/monitoring/MonitoringPage'
import { PendingTasksPage } from './features/tasks/PendingTasksPage'
import { MessageDialog } from './shared/components/MessageDialog'
import { PenTool, Database, ListTodo, Layers, BarChart2, Settings as SettingsIcon, Mail, AlertTriangle, Eye, History, Wifi, WifiOff, Loader2 } from 'lucide-react'
import { useToast } from '@/hooks/use-toast'
import { IncidentsPage } from './features/incidents/IncidentsPage'
import { OverviewPage } from './features/overview/OverviewPage'
import { HistoryPage } from './features/history/HistoryPage'
import { ProcessDefinitionPage } from './features/definitions/ProcessDefinitionPage'
import { cn } from '@/lib/utils'
import { useEngineStatus } from './shared/hooks/use-engine-status'
import { EngineOfflineBanner } from './shared/components/EngineOfflineBanner'

function App() {
  const { toast } = useToast()
  const { status: engineStatus, refresh: refreshEngine } = useEngineStatus(10_000)

  useEffect(() => {
    const saved = localStorage.getItem('theme')
    
    const applySystemTheme = () => {
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      document.documentElement.setAttribute('data-theme', prefersDark ? 'dark' : 'light');
    };

    if (!saved || saved === 'auto') {
      applySystemTheme();
      const mql = window.matchMedia('(prefers-color-scheme: dark)');
      mql.addEventListener('change', applySystemTheme);
      return () => mql.removeEventListener('change', applySystemTheme);
    } else {
      document.documentElement.setAttribute('data-theme', saved);
    }
  }, [])

  const [activeTab, setActiveTab] = useState('definitions')
  const [selectedInstanceId, setSelectedInstanceId] = useState<string | null>(null)
  const [selectedDefinitionKey, setSelectedDefinitionKey] = useState<string | null>(null)
  const [viewXml, setViewXml] = useState<string | null>(null)
  const [showMessageDialog, setShowMessageDialog] = useState(false)

  const handleDeploy = async (xml: string) => {
    try {
      const id = await deployDefinition(xml, 'modeler-process')
      toast({ description: "Deployed! Definition: " + id.substring(0, 8) })
    } catch (e: any) {
      toast({ variant: 'destructive', description: "Deploy failed: " + e })
    }
  }

  const handleStart = async (xml: string, variables: Record<string, unknown>): Promise<string> => {
    // Auto-deploy the current modeler state
    const newDefId = await deployDefinition(xml, 'modeler-process')

    // Detect timer start events: only if a <bpmn:startEvent> contains a timerEventDefinition
    const isTimerStart = /<bpmn:startEvent[^>]*>[\s\S]*?<bpmn:timerEventDefinition[\s\S]*?<\/bpmn:startEvent>/i.test(xml)
    const id = isTimerStart
      ? await startTimerInstance(newDefId, variables)
      : await startInstance(newDefId, variables)

    // Navigate to the new instance
    setSelectedInstanceId(id)
    setActiveTab('instances')

    return id
  }

  // Called when user clicks "View in Modeler" on a deployed definition
  const handleViewDefinition = (xml: string) => {
    setViewXml(xml)
    setActiveTab('modeler')
  }

  // Called when user clicks "New Diagram" in the Modeler
  const handleNewDiagram = () => {
    setViewXml(null)
  }

  // Called when the user opens a local BPMN file via "Open File".
  const handleOpenFile = () => {
    setViewXml(null)
  }

  const navItems = [
    { id: 'modeler',      icon: PenTool,        label: 'BPMN Modeler',        requiresEngine: false },
    { id: 'definitions',  icon: Database,        label: 'Deployed Processes',  requiresEngine: true  },
    { id: 'tasks',        icon: ListTodo,        label: 'Pending Tasks',       requiresEngine: true  },
    { id: 'incidents',    icon: AlertTriangle,   label: 'Incidents',           requiresEngine: true  },
    { id: 'overview',     icon: Eye,             label: 'Overview',            requiresEngine: true  },
    { id: 'history',      icon: History,         label: 'History',             requiresEngine: true  },
    { id: 'instances',    icon: Layers,          label: 'Instances',           requiresEngine: true,
      onClick: () => { setSelectedInstanceId(null); setActiveTab('instances'); }
    },
    { id: 'monitoring',   icon: BarChart2,       label: 'Monitoring',          requiresEngine: true  },
    { id: 'settings',     icon: SettingsIcon,    label: 'Settings',            requiresEngine: false },
  ];

  const activePageRequiresEngine = navItems.find(i => i.id === activeTab)?.requiresEngine ?? false
  const showOfflineBanner = activePageRequiresEngine && engineStatus === 'offline'

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      {/* SIDEBAR */}
      <div className="w-[250px] bg-muted/20 border-r flex flex-col flex-shrink-0">
        <div className="p-5 border-b bg-gradient-to-r from-blue-600 to-indigo-600 text-white">
          <div className="flex items-center gap-3">
            <img src="/logo.png" alt="BPMNinja Logo" className="h-16 w-16 object-contain drop-shadow-md rounded" />
            <div className="flex flex-col">
              <span className="text-2xl font-bold tracking-tight leading-none">BPMNinja</span>
              <span className="text-xs text-blue-200 mt-1 font-mono">v{__APP_VERSION__}</span>
            </div>
          </div>
          <span className="text-xs text-blue-200 mt-1 block">Workflow Engine</span>
        </div>
        
        <nav className="flex-1 flex flex-col pt-2 overflow-y-auto w-full">
          {navItems.map(item => {
            const isOffline = item.requiresEngine && engineStatus === 'offline'
            return (
              <button
                key={item.id}
                onClick={item.onClick || (() => setActiveTab(item.id))}
                title={isOffline ? 'Engine-Server nicht erreichbar' : undefined}
                className={cn(
                  "nav-item w-full flex items-center gap-3 px-5 py-3.5 text-sm font-medium transition-colors border-b border-transparent",
                  activeTab === item.id
                    ? "active bg-accent text-primary border-r-4 border-r-primary"
                    : "hover:bg-accent/50 hover:text-foreground text-muted-foreground",
                  isOffline && "opacity-40"
                )}
              >
                <item.icon className="h-4 w-4" />
                <span className="flex-1 text-left">{item.label}</span>
                {isOffline && <WifiOff className="h-3 w-3 opacity-60 flex-shrink-0" />}
              </button>
            )
          })}

          <button
            className={cn(
              "w-full flex items-center gap-3 px-5 py-3.5 text-sm font-medium transition-colors hover:bg-accent/50 mt-auto text-muted-foreground border-t",
              engineStatus === 'offline' && "opacity-40"
            )}
            onClick={() => setShowMessageDialog(true)}
            title={engineStatus === 'offline' ? 'Engine-Server nicht erreichbar' : undefined}
          >
            <Mail className="h-4 w-4" />
            Send Message
            {engineStatus === 'offline' && <WifiOff className="h-3 w-3 opacity-60 ml-auto flex-shrink-0" />}
          </button>
        </nav>

        {/* Engine-Status-Anzeige */}
        <div className="p-3 border-t bg-background/50 flex items-center gap-2">
          {engineStatus === 'checking' && (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground flex-shrink-0" />
              <span className="text-xs text-muted-foreground">Verbinde...</span>
            </>
          )}
          {engineStatus === 'online' && (
            <>
              <Wifi className="h-3.5 w-3.5 text-green-500 flex-shrink-0" />
              <span className="text-xs text-green-600 dark:text-green-500 font-medium">Engine Online</span>
            </>
          )}
          {engineStatus === 'offline' && (
            <>
              <WifiOff className="h-3.5 w-3.5 text-destructive flex-shrink-0" />
              <span className="text-xs text-destructive font-medium">Engine Offline</span>
              <button
                onClick={() => setActiveTab('settings')}
                className="ml-auto text-xs text-destructive underline underline-offset-2 hover:no-underline whitespace-nowrap"
              >
                Prüfen →
              </button>
            </>
          )}
        </div>
      </div>
      
      {/* MAIN CONTENT */}
      <div className="flex-1 flex flex-col relative overflow-hidden bg-background">
        {showOfflineBanner && (
          <EngineOfflineBanner onGoToSettings={() => setActiveTab('settings')} />
        )}
        <div className={cn("flex-1 flex flex-col h-full", activeTab === 'modeler' ? 'flex' : 'hidden')}>
          <ModelerPage onDeploy={handleDeploy} onStart={handleStart} onNewDiagram={handleNewDiagram} onOpenFile={handleOpenFile} initialXml={viewXml} />
        </div>

        {activeTab === 'definitions' && !selectedDefinitionKey && (
          <DeployedProcessesPage
            onView={handleViewDefinition}
            onViewInstance={(id: string) => { setSelectedInstanceId(id); setActiveTab('instances'); }}
            onViewDefinition={(key: string) => setSelectedDefinitionKey(key)}
          />
        )}

        {activeTab === 'definitions' && selectedDefinitionKey && (
          <ProcessDefinitionPage
            definitionKey={selectedDefinitionKey}
            onBack={() => setSelectedDefinitionKey(null)}
            onViewInstance={(id: string) => { setSelectedInstanceId(id); setSelectedDefinitionKey(null); setActiveTab('instances'); }}
          />
        )}

        {activeTab === 'tasks' && <PendingTasksPage />}
        {activeTab === 'incidents' && <IncidentsPage onViewInstance={(id: string) => { setSelectedInstanceId(id); setActiveTab('instances'); }} />}
        {activeTab === 'overview' && <OverviewPage onViewInstance={(id: string) => { setSelectedInstanceId(id); setActiveTab('instances'); }} />}
        {activeTab === 'history' && <HistoryPage onViewInstance={(id: string) => { setSelectedInstanceId(id); setActiveTab('instances'); }} />}
        
        {activeTab === 'instances' && (
          <InstancesPage 
            selectedInstanceId={selectedInstanceId} 
            onClearSelection={() => setSelectedInstanceId(null)} 
          />
        )}

        {activeTab === 'monitoring' && <MonitoringPage />}
        {activeTab === 'settings' && (
          <SettingsPage engineStatus={engineStatus} onConnectionChanged={refreshEngine} />
        )}
      </div>
      
      <MessageDialog open={showMessageDialog} onClose={() => setShowMessageDialog(false)} />
    </div>
  )
}

export default App

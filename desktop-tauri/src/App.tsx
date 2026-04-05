import { useState, useEffect } from 'react'
import { deployDefinition, startInstance } from './lib/tauri'
import { Modeler } from './Modeler'
import { Instances } from './Instances'
import { DeployedProcesses } from './DeployedProcesses'
import { Settings } from './Settings'
import { Monitoring } from './Monitoring'
import { PendingTasks } from './PendingTasks'
import { MessageDialog } from './MessageDialog'
import { PenTool, Database, ListTodo, Layers, BarChart2, Settings as SettingsIcon, Mail, AlertTriangle } from 'lucide-react'
import { useToast } from '@/hooks/use-toast'
import { IncidentsView } from './IncidentsView'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'

function App() {
  const { toast } = useToast()
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

    // Start instance with the freshly deployed definition
    const id = await startInstance(newDefId, variables)

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
    { id: 'modeler', icon: PenTool, label: 'BPMN Modeler' },
    { id: 'definitions', icon: Database, label: 'Deployed Processes' },
    { id: 'tasks', icon: ListTodo, label: 'Pending Tasks' },
    { id: 'incidents', icon: AlertTriangle, label: 'Incidents' },
    { id: 'instances', icon: Layers, label: 'Instances', 
      onClick: () => { setSelectedInstanceId(null); setActiveTab('instances'); } 
    },
    { id: 'monitoring', icon: BarChart2, label: 'Monitoring' },
    { id: 'settings', icon: SettingsIcon, label: 'Settings' },
  ];

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      {/* SIDEBAR */}
      <div className="w-[250px] bg-muted/20 border-r flex flex-col flex-shrink-0">
        <div className="p-5 border-b bg-gradient-to-r from-blue-600 to-indigo-600 text-white">
          <div className="flex items-center gap-3">
            <img src="/logo.png" alt="BPMNinja Logo" className="h-16 w-16 object-contain drop-shadow-md rounded" />
            <span className="text-2xl font-bold tracking-tight">BPMNinja</span>
          </div>
          <span className="text-xs text-blue-200 mt-1 block">Workflow Engine</span>
        </div>
        
        <nav className="flex-1 flex flex-col pt-2 overflow-y-auto w-full">
          {navItems.map(item => (
            <button
              key={item.id}
              onClick={item.onClick || (() => setActiveTab(item.id))}
              className={cn(
                "w-full flex items-center gap-3 px-5 py-3.5 text-sm font-medium transition-colors border-b border-transparent",
                activeTab === item.id 
                  ? "bg-accent text-primary border-r-4 border-r-primary pointer-events-none" 
                  : "hover:bg-accent/50 hover:text-foreground text-muted-foreground"
              )}
            >
              <item.icon className="h-4 w-4" />
              {item.label}
            </button>
          ))}

          <button 
            className="w-full flex items-center gap-3 px-5 py-3.5 text-sm font-medium transition-colors hover:bg-accent/50 mt-auto text-muted-foreground border-t"
            onClick={() => setShowMessageDialog(true)}
          >
            <Mail className="h-4 w-4" /> 
            Send Message
          </button>
        </nav>

        <div className="p-4 border-t bg-background/50">
          <Badge className="bg-green-600 hover:bg-green-700 text-white border-0 font-medium tracking-wide">
             <span className="mr-1.5 text-[0.65rem] leading-none">●</span> Thin Client
          </Badge>
        </div>
      </div>
      
      {/* MAIN CONTENT */}
      <div className="flex-1 flex flex-col relative overflow-hidden bg-background">
        <div className={cn("flex-1 flex flex-col h-full", activeTab === 'modeler' ? 'flex' : 'hidden')}>
          <Modeler onDeploy={handleDeploy} onStart={handleStart} onNewDiagram={handleNewDiagram} onOpenFile={handleOpenFile} initialXml={viewXml} />
        </div>

        {activeTab === 'definitions' && (
          <DeployedProcesses 
            onView={handleViewDefinition} 
            onViewInstance={(id) => { setSelectedInstanceId(id); setActiveTab('instances'); }}
          />
        )}

        {activeTab === 'tasks' && <PendingTasks />}
        {activeTab === 'incidents' && <IncidentsView onViewInstance={(id) => { setSelectedInstanceId(id); setActiveTab('instances'); }} />}
        
        {activeTab === 'instances' && (
          <Instances 
            selectedInstanceId={selectedInstanceId} 
            onClearSelection={() => setSelectedInstanceId(null)} 
          />
        )}

        {activeTab === 'monitoring' && <Monitoring />}
        {activeTab === 'settings' && <Settings />}
      </div>
      
      <MessageDialog open={showMessageDialog} onClose={() => setShowMessageDialog(false)} />
    </div>
  )
}

export default App

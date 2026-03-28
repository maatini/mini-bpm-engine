import { useState, useEffect } from 'react'
import { deployDefinition, startInstance, getPendingTasks, completeTask, getPendingServiceTasks, fetchAndLockServiceTasks, completeServiceTask, type PendingUserTask, type PendingServiceTask } from './lib/tauri'
import { Modeler } from './Modeler'
import { Instances } from './Instances'
import { DeployedProcesses } from './DeployedProcesses'
import { Settings } from './Settings'
import { Monitoring } from './Monitoring'
import { PenTool, Database, ListTodo, Layers, BarChart2, Settings as SettingsIcon } from 'lucide-react'

function App() {
  const [activeTab, setActiveTab] = useState('definitions')
  const [selectedInstanceId, setSelectedInstanceId] = useState<string | null>(null)
  const [tasks, setTasks] = useState<PendingUserTask[]>([])
  const [serviceTasks, setServiceTasks] = useState<PendingServiceTask[]>([])
  const [viewXml, setViewXml] = useState<string | null>(null)


  useEffect(() => {
    if (activeTab === 'tasks') {
      fetchTasks()
    }
  }, [activeTab])

  const fetchTasks = async () => {
    try {
      const [pending, pendingServices] = await Promise.all([
        getPendingTasks(),
        getPendingServiceTasks()
      ])
      setTasks(pending)
      setServiceTasks(pendingServices)
    } catch (e) {
      console.error("Failed to fetch tasks", e)
    }
  }

  const handleDeploy = async (xml: string) => {
    try {
      const id = await deployDefinition(xml, 'modeler-process')
      alert("Deployed definition! ID: " + id)
    } catch (e) {
      alert("Error deploying: " + e)
    }
  }

  const handleStart = async (xml: string, variables: Record<string, unknown>) => {
    try {
      // Auto-deploy the current modeler state
      const newDefId = await deployDefinition(xml, 'modeler-process')

      // Start instance with the freshly deployed definition
      const id = await startInstance(newDefId, variables)

      // Navigate to the new instance
      setSelectedInstanceId(id)
      setActiveTab('instances')
    } catch (e) {
      alert("Error deploying/starting: " + e)
    }
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

  const handleComplete = async (taskId: string) => {
    try {
      await completeTask(taskId)
      fetchTasks()
      alert("Task completed!")
    } catch (e) {
      alert("Error completing task: " + e)
    }
  }

  const handleCompleteServiceTask = async (task: PendingServiceTask) => {
    try {
      if (!task.worker_id) {
        // Automatically fetch and lock the specific task's topic first
        const lockedTasks = await fetchAndLockServiceTasks("tauri-ui", 10, task.topic, 5000)
        if (!lockedTasks.some(t => t.id === task.id)) {
          alert("Could not lock task! It might have been acquired by another worker.")
          fetchTasks()
          return
        }
      } else if (task.worker_id !== "tauri-ui") {
        alert("Task is currently locked by another worker: " + task.worker_id)
        return
      }

      await completeServiceTask(task.id, "tauri-ui")
      fetchTasks()
      alert("Service Task completed!")
    } catch (e) {
      alert("Error completing service task: " + e)
    }
  }

  return (
    <div className="app-container">
      <div className="sidebar">
        <div className="sidebar-header">Mini BPM</div>
        <div className={`nav-item ${activeTab === 'modeler' ? 'active' : ''}`} onClick={() => setActiveTab('modeler')} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <PenTool size={18} /> BPMN Modeler
        </div>
        <div className={`nav-item ${activeTab === 'definitions' ? 'active' : ''}`} onClick={() => setActiveTab('definitions')} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <Database size={18} /> Deployed Processes
        </div>
        <div className={`nav-item ${activeTab === 'tasks' ? 'active' : ''}`} onClick={() => setActiveTab('tasks')} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <ListTodo size={18} /> Pending Tasks
        </div>
        <div className={`nav-item ${activeTab === 'instances' ? 'active' : ''}`} onClick={() => { setSelectedInstanceId(null); setActiveTab('instances'); }} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <Layers size={18} /> Instances
        </div>
        <div className={`nav-item ${activeTab === 'monitoring' ? 'active' : ''}`} onClick={() => setActiveTab('monitoring')} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <BarChart2 size={18} /> Monitoring
        </div>
        <div className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <SettingsIcon size={18} /> Settings
        </div>

        <div className="sidebar-footer">
          <span className="backend-badge backend-nats">
            ● Thin Client
          </span>
        </div>
      </div>
      
      <div className="main-content">
        {activeTab === 'modeler' && (
          <Modeler onDeploy={handleDeploy} onStart={handleStart} onNewDiagram={handleNewDiagram} onOpenFile={handleOpenFile} initialXml={viewXml} />
        )}

        {activeTab === 'definitions' && (
          <DeployedProcesses 
            onView={handleViewDefinition} 
            onViewInstance={(id) => { setSelectedInstanceId(id); setActiveTab('instances'); }}
          />
        )}

        {activeTab === 'tasks' && (
          <div>
            <h2>Pending Tasks</h2>
            <div className="header-actions">
              <button className="button" onClick={fetchTasks}>Refresh</button>
            </div>
            
            <h3 style={{ marginTop: 24, marginBottom: 16 }}>User Tasks</h3>
            {tasks.length === 0 && <div style={{marginLeft: 20}}>No pending user tasks.</div>}
            <div style={{ display: 'grid', gridTemplateColumns: '1fr', gap: '16px' }}>
              {tasks.map(task => (
                <div key={task.task_id} className="card">
                  <div className="card-title">Task: {task.node_id}</div>
                  <div>Assignee: {task.assignee}</div>
                  <div>Instance: {task.instance_id}</div>
                  <div style={{marginTop: 10}}>
                    <button className="button" onClick={() => handleComplete(task.task_id)}>Complete Task</button>
                  </div>
                </div>
              ))}
            </div>

            <h3 style={{ marginTop: 32, marginBottom: 16 }}>Service Tasks (External)</h3>
            {serviceTasks.length === 0 && <div style={{marginLeft: 20}}>No pending service tasks.</div>}
            <div style={{ display: 'grid', gridTemplateColumns: '1fr', gap: '16px' }}>
              {serviceTasks.map(task => (
                <div key={task.id} className="card" style={{ borderLeft: '4px solid #8b5cf6' }}>
                  <div className="card-title" style={{ display: 'flex', justifyContent: 'space-between' }}>
                    <span>Task: {task.node_id}</span>
                    <span style={{ fontSize: '0.8rem', padding: '2px 8px', background: '#eedeff', color: '#6d28d9', borderRadius: '12px', fontWeight: 600 }}>
                      Topic: {task.topic}
                    </span>
                  </div>
                  <div>Worker: {task.worker_id || <span style={{ color: '#94a3b8', fontStyle: 'italic' }}>Unlocked</span>}</div>
                  <div>Instance: {task.instance_id}</div>
                  <div>Retries left: {task.retries}</div>
                  {task.error_message && (
                    <div style={{ color: '#ef4444', fontSize: '0.9rem', marginTop: 4 }}>Error: {task.error_message}</div>
                  )}
                  <div style={{marginTop: 10}}>
                    <button className="button" style={{ background: '#8b5cf6' }} onClick={() => handleCompleteServiceTask(task)}>
                      Complete as 'tauri-ui'
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {activeTab === 'instances' && (
          <Instances selectedInstanceId={selectedInstanceId} />
        )}

        {activeTab === 'monitoring' && (
          <Monitoring />
        )}

        {activeTab === 'settings' && (
          <Settings />
        )}
      </div>
    </div>
  )
}

export default App


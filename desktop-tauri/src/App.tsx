import { useState, useEffect } from 'react'
import { deployDefinition, startInstance, getPendingTasks, completeTask, getBackendInfo, type PendingUserTask, type BackendInfo } from './lib/tauri'
import { Modeler } from './Modeler'
import { Instances } from './Instances'
import { DeployedProcesses } from './DeployedProcesses'
import { Settings } from './Settings'
import { Monitoring } from './Monitoring'

function App() {
  const [activeTab, setActiveTab] = useState('modeler')
  const [tasks, setTasks] = useState<PendingUserTask[]>([])
  const [defId, setDefId] = useState<string>('')
  const [viewXml, setViewXml] = useState<string | null>(null)
  const [backendInfo, setBackendInfo] = useState<BackendInfo | null>(null)

  useEffect(() => {
    getBackendInfo().then(setBackendInfo).catch(console.error)
  }, [])

  useEffect(() => {
    if (activeTab === 'tasks') {
      fetchTasks()
    }
  }, [activeTab])

  const fetchTasks = async () => {
    try {
      const pending = await getPendingTasks()
      setTasks(pending)
    } catch (e) {
      console.error("Failed to fetch tasks", e)
    }
  }

  const handleDeploy = async (xml: string) => {
    try {
      const id = await deployDefinition(xml, 'modeler-process')
      setDefId(id)
      alert("Deployed definition! ID: " + id)
    } catch (e) {
      alert("Error deploying: " + e)
    }
  }

  const handleStart = async (variables: Record<string, unknown>) => {
    if (!defId) {
      alert("Please deploy a process first.")
      return
    }
    try {
      const id = await startInstance(defId, variables)
      alert("Started instance! ID: " + id)
    } catch (e) {
      alert("Error starting: " + e)
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
    setDefId('')
  }

  // Called when the user opens a local BPMN file via "Open File".
  const handleOpenFile = () => {
    setViewXml(null)
    setDefId('')
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

  return (
    <div className="app-container">
      <div className="sidebar">
        <div className="sidebar-header">Mini BPM</div>
        <div className={`nav-item ${activeTab === 'modeler' ? 'active' : ''}`} onClick={() => setActiveTab('modeler')}>
          BPMN Modeler
        </div>
        <div className={`nav-item ${activeTab === 'definitions' ? 'active' : ''}`} onClick={() => setActiveTab('definitions')}>
          Deployed Processes
        </div>
        <div className={`nav-item ${activeTab === 'tasks' ? 'active' : ''}`} onClick={() => setActiveTab('tasks')}>
          Pending Tasks
        </div>
        <div className={`nav-item ${activeTab === 'instances' ? 'active' : ''}`} onClick={() => setActiveTab('instances')}>
          Instances
        </div>
        <div className={`nav-item ${activeTab === 'monitoring' ? 'active' : ''}`} onClick={() => setActiveTab('monitoring')}>
          📊 Monitoring
        </div>
        <div className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`} onClick={() => setActiveTab('settings')}>
          ⚙ Settings
        </div>

        {/* Sidebar footer: backend badge */}
        <div className="sidebar-footer">
          <span className={`backend-badge ${backendInfo?.backend_type === 'nats' ? 'backend-nats' : 'backend-inmemory'}`}>
            {backendInfo ? (backendInfo.backend_type === 'nats' ? '● NATS' : '● In-Memory') : '…'}
          </span>
        </div>
      </div>
      
      <div className="main-content">
        {activeTab === 'modeler' && (
          <Modeler onDeploy={handleDeploy} onStart={handleStart} onNewDiagram={handleNewDiagram} onOpenFile={handleOpenFile} initialXml={viewXml} />
        )}

        {activeTab === 'definitions' && (
          <DeployedProcesses onView={handleViewDefinition} />
        )}

        {activeTab === 'tasks' && (
          <div>
            <h2>Pending Tasks</h2>
            <div className="header-actions">
              <button className="button" onClick={fetchTasks}>Refresh</button>
            </div>
            {tasks.length === 0 && <div style={{marginLeft: 20}}>No pending tasks.</div>}
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
        )}

        {activeTab === 'instances' && (
          <Instances />
        )}

        {activeTab === 'monitoring' && (
          <Monitoring />
        )}

        {activeTab === 'settings' && (
          <Settings onBackendChange={(info) => setBackendInfo(info)} />
        )}
      </div>
    </div>
  )
}

export default App


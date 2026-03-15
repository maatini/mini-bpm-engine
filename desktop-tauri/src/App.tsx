import { useState, useEffect } from 'react'
import { deployDefinition, startInstance, getPendingTasks, completeTask, type PendingUserTask } from './lib/tauri'
import { Modeler } from './Modeler'
import { Instances } from './Instances'

function App() {
  const [activeTab, setActiveTab] = useState('modeler')
  const [tasks, setTasks] = useState<PendingUserTask[]>([])
  const [defId, setDefId] = useState<string>('')

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

  const handleStart = async () => {
    if (!defId) {
      alert("Please deploy a process first.")
      return
    }
    try {
      const id = await startInstance(defId)
      alert("Started instance! ID: " + id)
    } catch (e) {
      alert("Error starting: " + e)
    }
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
        <div className={`nav-item ${activeTab === 'tasks' ? 'active' : ''}`} onClick={() => setActiveTab('tasks')}>
          Pending Tasks
        </div>
        <div className={`nav-item ${activeTab === 'instances' ? 'active' : ''}`} onClick={() => setActiveTab('instances')}>
          Instances
        </div>
      </div>
      
      <div className="main-content">
        {activeTab === 'modeler' && (
          <Modeler onDeploy={handleDeploy} onStart={handleStart} />
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
      </div>
    </div>
  )
}

export default App

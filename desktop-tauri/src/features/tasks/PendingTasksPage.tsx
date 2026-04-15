import { useState, useEffect, useRef } from 'react';
import { getPendingTasks, getPendingServiceTasks, completeTask, 
         fetchAndLockServiceTasks, completeServiceTask, 
         type PendingUserTask, type PendingServiceTask } from '../../shared/lib/tauri';
import { useToast } from '@/hooks/use-toast';
import { VariableEditor, type VariableRow, serializeVariables } from '../../shared/components/VariableEditor';
import { RefreshCw, UserCircle, Briefcase, CheckCircle2, Play } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from '@/components/ui/dialog';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { Skeleton } from '@/components/ui/skeleton';

export function PendingTasksPage() {
  const { toast } = useToast();
  const [tasks, setTasks] = useState<PendingUserTask[]>([]);
  const [serviceTasks, setServiceTasks] = useState<PendingServiceTask[]>([]);
  const [loading, setLoading] = useState(true);
  
  // Complete dialog state
  const [completingTask, setCompletingTask] = useState<PendingUserTask | null>(null);
  const [completeVars, setCompleteVars] = useState<VariableRow[]>([]);

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchTasks = async () => {
    try {
      const [pending, pendingServices] = await Promise.all([
        getPendingTasks(),
        getPendingServiceTasks()
      ]);
      setTasks(pending);
      setServiceTasks(pendingServices);
    } catch (e: any) {
      console.error("Failed to fetch tasks", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchTasks();
    intervalRef.current = setInterval(fetchTasks, 3000);
    return () => { if (intervalRef.current) clearInterval(intervalRef.current); };
  }, []);

  const handleCompleteClick = (task: PendingUserTask) => {
    setCompletingTask(task);
    setCompleteVars([]);  // Empty — user adds vars as needed
  };

  const handleCompleteConfirm = async () => {
    if (!completingTask) return;
    const vars = serializeVariables(completeVars);
    if (vars === null) {
      toast({ variant: 'destructive', description: 'Invalid variables format. Please check JSON or Numbers.' });
      return; // Validation failed
    }

    try {
      await completeTask(completingTask.task_id, vars);
      toast({ description: 'Task completed successfully!' });
      setCompletingTask(null);
      fetchTasks();
    } catch (e: any) {
      toast({ variant: 'destructive', description: 'Error: ' + e });
    }
  };

  const handleCompleteServiceTask = async (task: PendingServiceTask) => {
    try {
      if (!task.worker_id) {
        // Automatically fetch and lock the specific task's topic first
        const lockedTasks = await fetchAndLockServiceTasks("tauri-ui", 10, task.topic, 5000)
        if (!lockedTasks.some(t => t.id === task.id)) {
          toast({ variant: 'destructive', description: "Could not lock task! It might have been acquired by another worker." })
          fetchTasks()
          return
        }
      } else if (task.worker_id !== "tauri-ui") {
        toast({ variant: 'destructive', description: "Task is currently locked by another worker: " + task.worker_id })
        return
      }

      await completeServiceTask(task.id, "tauri-ui")
      fetchTasks()
      toast({ description: "Service Task completed!" })
    } catch (e: any) {
      toast({ variant: 'destructive', description: "Error completing service task: " + e })
    }
  }

  return (
    <div className="flex flex-col h-full bg-background">
      <div className="flex items-center justify-between px-6 py-4 border-b bg-background">
        <h2 className="text-2xl font-bold tracking-tight">Pending Tasks</h2>
        <div className="flex items-center gap-4">
          <span className="text-xs text-muted-foreground">Auto-refreshing</span>
          <Button onClick={fetchTasks} variant="outline" size="sm" className="gap-2">
            <RefreshCw className="h-4 w-4" /> Refresh
          </Button>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-8">
          
          {/* USER TASKS */}
          <section>
            <div className="flex items-center gap-2 mb-4">
              <UserCircle className="h-5 w-5 text-primary" />
              <h3 className="text-lg font-semibold tracking-tight">User Tasks</h3>
              <Badge variant="secondary" className="ml-2">{tasks.length}</Badge>
            </div>
            
            {loading ? (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                {[1,2,3,4].map(i => (
                  <Card key={i} className="flex flex-col h-[180px]">
                    <div className="p-4 flex-1">
                      <Skeleton className="h-6 w-[140px] mb-4" />
                      <Skeleton className="h-4 w-full mb-2" />
                      <Skeleton className="h-4 w-[100px]" />
                    </div>
                    <div className="p-4 border-t">
                      <Skeleton className="h-9 w-full" />
                    </div>
                  </Card>
                ))}
              </div>
            ) : tasks.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-10 text-center border border-dashed rounded-lg bg-muted/10">
                <UserCircle className="h-12 w-12 text-muted-foreground/30 mb-3" />
                <h3 className="text-base font-semibold text-muted-foreground">No Pending User Tasks</h3>
                <p className="text-sm text-muted-foreground/70 mt-1">
                  You're all caught up!
                </p>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
              {tasks.map(task => (
                <Card key={task.task_id} className="card flex flex-col">
                  <CardHeader className="pb-3">
                    <CardTitle className="text-base flex items-center justify-between">
                      Node: <span className="font-mono text-primary">{task.node_id}</span>
                    </CardTitle>
                  </CardHeader>
                  <CardContent className="text-sm space-y-2 flex-1">
                    <div className="flex justify-between border-b pb-2">
                      <span className="text-muted-foreground">Assignee</span>
                      <span className="font-medium">{task.assignee || 'Unassigned'}</span>
                    </div>
                    <div className="flex flex-col pt-1">
                      <span className="text-muted-foreground text-xs mb-1">Instance ID</span>
                      <span className="font-mono text-xs">{task.instance_id.substring(0, 8)}…</span>
                    </div>
                    {task.business_key && (
                      <div className="flex flex-col pt-1">
                        <span className="text-muted-foreground text-xs mb-1">Business Key</span>
                        <span className="text-xs font-medium">{task.business_key}</span>
                      </div>
                    )}
                  </CardContent>
                  <CardFooter className="pt-2">
                    <Button className="w-full gap-2" onClick={() => handleCompleteClick(task)}>
                      <CheckCircle2 className="h-4 w-4" /> Complete Task
                    </Button>
                  </CardFooter>
                </Card>
              ))}
            </div>
            )}
          </section>

          <Separator />

          {/* SERVICE TASKS */}
          <section>
            <div className="flex items-center gap-2 mb-4">
              <Briefcase className="h-5 w-5 text-purple-600" />
              <h3 className="text-lg font-semibold tracking-tight">Service Tasks (External)</h3>
              <Badge variant="secondary" className="ml-2">{serviceTasks.length}</Badge>
            </div>
            
            {loading ? (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                {[1,2,3,4].map(i => (
                  <Card key={i} className="flex flex-col h-[200px] border-l-4 border-l-purple-500/30">
                    <div className="p-4 flex-1">
                      <Skeleton className="h-6 w-[180px] mb-4" />
                      <Skeleton className="h-4 w-full mb-2" />
                      <Skeleton className="h-4 w-[120px]" />
                    </div>
                    <div className="p-4 border-t">
                      <Skeleton className="h-9 w-full" />
                    </div>
                  </Card>
                ))}
              </div>
            ) : serviceTasks.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-10 text-center border border-dashed rounded-lg bg-muted/10">
                <Briefcase className="h-12 w-12 text-muted-foreground/30 mb-3" />
                <h3 className="text-base font-semibold text-muted-foreground">No Pending Service Tasks</h3>
                <p className="text-sm text-muted-foreground/70 mt-1">
                  No automated workers are required right now.
                </p>
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
              {serviceTasks.map(task => (
                <Card key={task.id} className="card border-l-4 border-l-purple-500 flex flex-col">
                  <CardHeader className="pb-3">
                    <div className="flex items-center justify-between">
                       <span className="font-mono text-sm font-semibold">{task.node_id}</span>
                       <Badge variant="secondary" className="bg-purple-100 text-purple-700 hover:bg-purple-100/80 dark:bg-purple-500/10 dark:text-purple-400">
                         {task.topic}
                       </Badge>
                    </div>
                  </CardHeader>
                  <CardContent className="text-sm flex-1 space-y-2">
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Worker</span>
                      {task.worker_id ? (
                        <span className="font-medium text-foreground">{task.worker_id}</span>
                      ) : (
                        <span className="text-muted-foreground italic text-xs">Unlocked</span>
                      )}
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Instance</span>
                      <span className="font-mono text-xs">{task.instance_id.substring(0, 8)}…</span>
                    </div>
                    {task.business_key && (
                      <div className="flex justify-between">
                        <span className="text-muted-foreground">Business Key</span>
                        <span className="font-medium text-xs">{task.business_key}</span>
                      </div>
                    )}
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Retries left</span>
                      <span className="font-medium">{task.retries}</span>
                    </div>
                    {task.error_message && (
                      <div className="mt-2 p-2 bg-destructive/10 text-destructive rounded-md text-xs">
                        Error: {task.error_message}
                      </div>
                    )}
                  </CardContent>
                  <CardFooter className="pt-2">
                    <Button 
                      className="w-full gap-2 bg-purple-600 hover:bg-purple-700" 
                      onClick={() => handleCompleteServiceTask(task)}
                    >
                      <Play className="h-4 w-4" /> Complete as 'tauri-ui'
                    </Button>
                  </CardFooter>
                </Card>
              ))}
            </div>
            )}
          </section>

        </div>
      </ScrollArea>

      {/* Complete Task Dialog */}
      <Dialog open={!!completingTask} onOpenChange={(open) => !open && setCompletingTask(null)}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>Complete Task</DialogTitle>
            <DialogDescription>
              Task ID: <span className="font-mono text-primary">{completingTask?.node_id}</span>
            </DialogDescription>
          </DialogHeader>
          
          <div className="py-4">
             <h4 className="text-sm font-semibold mb-3">Add output variables (optional):</h4>
             <div className="bg-muted/30 border rounded-md p-4">
               <VariableEditor
                variables={completeVars}
                onChange={setCompleteVars}
               />
             </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setCompletingTask(null)}>Cancel</Button>
            <Button onClick={handleCompleteConfirm} className="gap-2 bg-green-600 hover:bg-green-700 text-white">
              <CheckCircle2 className="h-4 w-4" /> Complete Task
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

import re
import os

with open(".agent/skills/engine-core/SKILL.md", "r") as f:
    c = f.read()

c = c.replace("BpmnElement` (19 variants)", "BpmnElement` (21 variants)")
c = c.replace("SubProcess { called_element }", "EmbeddedSubProcess")
c = c.replace("InstanceState` (9 variants)", "InstanceState` (10 variants)")
c = c.replace("NextAction` (8 variants)", "NextAction` (14 variants)")
c = c.replace("Supported BPMN Elements (19 variants)", "Supported BPMN Elements (21 variants)")
c = c.replace("**ServiceTask { topic }**", "**ServiceTask { topic, multi_instance }**")
c = c.replace("**UserTask(assignee)**", "**UserTask(assignee, multi_instance)**")
c = c.replace("**ScriptTask { script }**", "**ScriptTask { script, multi_instance }**")
c = c.replace("**SendTask { message_name }**", "**SendTask { message_name, multi_instance }**")
c = c.replace("engine/stress_tests.rs", "engine/stress_tests.rs\n| `engine/definition_ops.rs` | Ops |\n| `engine/instance_ops.rs` | Ops |\n| `engine/message_processor.rs` | Ops |\n| `engine/persistence_ops.rs` | Ops |\n| `engine/process_start.rs` | Ops |\n| `engine/retry_queue.rs` | Ops |\n| `engine/timer_processor.rs` | Ops |\n| `engine/user_task.rs` | Ops |")

with open(".agent/skills/engine-core/SKILL.md", "w") as f:
    f.write(c)

with open(".agent/skills/bpmn-modeling/SKILL.md", "r") as f:
    c = f.read()

c = c.replace("`<subProcess>` (generic)", "`<subProcess>` | `EmbeddedSubProcess` & `SubProcessEndEvent` (using `flatten_subprocess()`)")
c = c.replace("Event sub-processes are detected and logged but handled separately", "Event sub-processes and nested Embedded Sub-Processes are recursively **flattened** into the single main workflow graph via `flatten_subprocess()`, retaining their logical scopes.")

# Add `<multiInstanceLoopCharacteristics>`
if "<multiInstanceLoopCharacteristics>" not in c:
    c = c.replace("`<task>` (generic) | `ServiceTask { topic: name }` (fallback) |", "`<task>` (generic) | `ServiceTask { topic: name }` (fallback) |\n| `<multiInstanceLoopCharacteristics>` | `multi_instance: Option<MultiInstanceDef>` (Parallel/Sequential loops on tasks) |")

with open(".agent/skills/bpmn-modeling/SKILL.md", "w") as f:
    f.write(c)

with open(".agent/skills/engine-server/SKILL.md", "r") as f:
    c = f.read()

c = c.replace("use `State<Arc<AppState>>` with `RwLock<WorkflowEngine>`", "use `State<Arc<AppState>>` with an internal `DashMap` (no external `RwLock<WorkflowEngine>` needed, just `Arc<WorkflowEngine>`)")

with open(".agent/skills/engine-server/SKILL.md", "w") as f:
    f.write(c)

print("Phase 2 done")

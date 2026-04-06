import re
import os

with open(".agent/rules/BPMN_WORKFLOW_ENGINE.md", "r") as f:
    c = f.read()

c = c.replace("## Supported BPMN Elements (19 variants)", "## Supported BPMN Elements (21 variants)")
c = c.replace("- **ServiceTask { topic }**", "- **ServiceTask { topic, multi_instance }**")
c = c.replace("- **UserTask(assignee)**", "- **UserTask { assignee, multi_instance }**")
c = c.replace("- **ScriptTask { script }**", "- **ScriptTask { script, multi_instance }**")
c = c.replace("- **SendTask { message_name }**", "- **SendTask { message_name, multi_instance }**")
c = c.replace("- **SubProcess { called_element }** — embedded sub-process with inline definition", "- **EmbeddedSubProcess { start_node_id }** — embedded sub-process with flattened definition\n- **SubProcessEndEvent { sub_process_id }** — internal end event for scope completion")
c = c.replace("BpmnElement` enum — all 19 variants", "BpmnElement` enum — all 21 variants")
c = c.replace("engine/tests.rs` — Comprehensive integration tests\n- `engine/stress_tests.rs` — Concurrency and load stress tests", "engine/tests.rs` — Comprehensive integration tests\n- `engine/stress_tests.rs` — Concurrency and load stress tests\n- `engine/definition_ops.rs`, `engine/instance_ops.rs`, `engine/message_processor.rs`, `engine/persistence_ops.rs`, `engine/process_start.rs`, `engine/retry_queue.rs`, `engine/timer_processor.rs`, `engine/user_task.rs` — Workflow state mutations")
c = c.replace("**`InstanceState` enum:**\n- `Running`", "**`InstanceState` enum (10 variants):**\n- `Running`")
c = c.replace("- `Completed`", "- `Completed`\n- `CompletedWithError { error_code }`\n- `WaitingOnEventBasedGateway`")
c = c.replace("**`NextAction` enum:**\n- `Continue(Token), `ContinueMultiple(Vec<Token>), `WaitForUser(PendingUserTask), `WaitForServiceTask(PendingServiceTask), `WaitForJoin { gateway_id, token }, `WaitForTimer(PendingTimer), `WaitForMessage(PendingMessageCatch), `Complete`", "**`NextAction` enum (14 variants):**\n- `Continue(Token), ContinueMultiple(Vec<Token>), WaitForUser(PendingUserTask), WaitForServiceTask(PendingServiceTask), WaitForJoin { gateway_id, token }, WaitForTimer(PendingTimer), WaitForMessage(PendingMessageCatch), Complete, WaitForEventGroup(Vec<NextAction>), ErrorEnd { error_code }, Terminate, WaitForCallActivity { called_element, token }, MultiInstanceFork { node_id, tokens }, MultiInstanceNext { node_id, token }")
c = c.replace("**`NextAction` enum:**\n- `Continue(Token)", "**`NextAction` enum (14 variants):**\n- `Continue(Token), ContinueMultiple, WaitForUser, WaitForServiceTask, WaitForJoin, WaitForTimer, WaitForMessage, Complete, WaitForEventGroup, ErrorEnd, Terminate, WaitForCallActivity, MultiInstanceFork, MultiInstanceNext")

with open(".agent/rules/BPMN_WORKFLOW_ENGINE.md", "w") as f:
    f.write(c)

with open(".agent/rules/RUST_ENGINE_AGENT.md", "r") as f:
    c = f.read()

c = c.replace("BpmnElement` (19 variants)", "BpmnElement` (21 variants)")
c = c.replace("InstanceState` (9 variants)", "InstanceState` (10 variants)")
c = c.replace("NextAction` (8 variants)", "NextAction` (14 variants)")
c = c.replace("engine/stress_tests.rs", "engine/stress_tests.rs\n  - `engine/definition_ops.rs`, `engine/instance_ops.rs`, `engine/message_processor.rs`, `engine/persistence_ops.rs`, `engine/process_start.rs`, `engine/retry_queue.rs`, `engine/timer_processor.rs`, `engine/user_task.rs`")

with open(".agent/rules/RUST_ENGINE_AGENT.md", "w") as f:
    f.write(c)

with open(".agent/rules/ORCHESTRATOR_AGENT.md", "r") as f:
    c = f.read()

c = c.replace("Contains `main()` for the headless backend.", "Is currently a stub and does not contain the backend's main (which lives in engine-server).")

with open(".agent/rules/ORCHESTRATOR_AGENT.md", "w") as f:
    f.write(c)

with open(".agent/rules/RUST_AGENT_RULES.md", "r") as f:
    c = f.read()

c = c.replace("cargo clippy --workspace --all-targets --all-features -- -D warnings", "cargo clippy --workspace --all-targets -- -D warnings")

with open(".agent/rules/RUST_AGENT_RULES.md", "w") as f:
    f.write(c)

print("Phase 1 done")

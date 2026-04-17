# Graph Report - .  (2026-04-17)

## Corpus Check
- 193 files · ~303,681 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1279 nodes · 3809 edges · 109 communities detected
- Extraction: 37% EXTRACTED · 63% INFERRED · 0% AMBIGUOUS · INFERRED: 2381 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]
- [[_COMMUNITY_Community 17|Community 17]]
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 19|Community 19]]
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 24|Community 24]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]
- [[_COMMUNITY_Community 44|Community 44]]
- [[_COMMUNITY_Community 45|Community 45]]
- [[_COMMUNITY_Community 46|Community 46]]
- [[_COMMUNITY_Community 47|Community 47]]
- [[_COMMUNITY_Community 48|Community 48]]
- [[_COMMUNITY_Community 49|Community 49]]
- [[_COMMUNITY_Community 50|Community 50]]
- [[_COMMUNITY_Community 51|Community 51]]
- [[_COMMUNITY_Community 52|Community 52]]
- [[_COMMUNITY_Community 53|Community 53]]
- [[_COMMUNITY_Community 54|Community 54]]
- [[_COMMUNITY_Community 55|Community 55]]
- [[_COMMUNITY_Community 56|Community 56]]
- [[_COMMUNITY_Community 57|Community 57]]
- [[_COMMUNITY_Community 58|Community 58]]
- [[_COMMUNITY_Community 59|Community 59]]
- [[_COMMUNITY_Community 60|Community 60]]
- [[_COMMUNITY_Community 61|Community 61]]
- [[_COMMUNITY_Community 62|Community 62]]
- [[_COMMUNITY_Community 63|Community 63]]
- [[_COMMUNITY_Community 64|Community 64]]
- [[_COMMUNITY_Community 65|Community 65]]
- [[_COMMUNITY_Community 66|Community 66]]
- [[_COMMUNITY_Community 67|Community 67]]
- [[_COMMUNITY_Community 68|Community 68]]
- [[_COMMUNITY_Community 69|Community 69]]
- [[_COMMUNITY_Community 70|Community 70]]
- [[_COMMUNITY_Community 71|Community 71]]
- [[_COMMUNITY_Community 72|Community 72]]
- [[_COMMUNITY_Community 73|Community 73]]
- [[_COMMUNITY_Community 74|Community 74]]
- [[_COMMUNITY_Community 75|Community 75]]
- [[_COMMUNITY_Community 76|Community 76]]
- [[_COMMUNITY_Community 77|Community 77]]
- [[_COMMUNITY_Community 78|Community 78]]
- [[_COMMUNITY_Community 79|Community 79]]
- [[_COMMUNITY_Community 80|Community 80]]
- [[_COMMUNITY_Community 81|Community 81]]
- [[_COMMUNITY_Community 82|Community 82]]
- [[_COMMUNITY_Community 83|Community 83]]
- [[_COMMUNITY_Community 84|Community 84]]
- [[_COMMUNITY_Community 85|Community 85]]
- [[_COMMUNITY_Community 86|Community 86]]
- [[_COMMUNITY_Community 87|Community 87]]
- [[_COMMUNITY_Community 88|Community 88]]
- [[_COMMUNITY_Community 89|Community 89]]
- [[_COMMUNITY_Community 90|Community 90]]
- [[_COMMUNITY_Community 91|Community 91]]
- [[_COMMUNITY_Community 92|Community 92]]
- [[_COMMUNITY_Community 93|Community 93]]
- [[_COMMUNITY_Community 94|Community 94]]
- [[_COMMUNITY_Community 95|Community 95]]
- [[_COMMUNITY_Community 96|Community 96]]
- [[_COMMUNITY_Community 97|Community 97]]
- [[_COMMUNITY_Community 98|Community 98]]
- [[_COMMUNITY_Community 99|Community 99]]
- [[_COMMUNITY_Community 100|Community 100]]
- [[_COMMUNITY_Community 101|Community 101]]
- [[_COMMUNITY_Community 102|Community 102]]
- [[_COMMUNITY_Community 103|Community 103]]
- [[_COMMUNITY_Community 104|Community 104]]
- [[_COMMUNITY_Community 105|Community 105]]
- [[_COMMUNITY_Community 106|Community 106]]
- [[_COMMUNITY_Community 107|Community 107]]
- [[_COMMUNITY_Community 108|Community 108]]

## God Nodes (most connected - your core abstractions)
1. `deploy_definition()` - 135 edges
2. `start_instance()` - 108 edges
3. `parse_bpmn_xml()` - 46 edges
4. `InMemoryPersistence` - 36 edges
5. `NatsPersistence` - 34 edges
6. `spawn()` - 26 edges
7. `main()` - 25 edges
8. `Value` - 25 edges
9. `parse_uuid()` - 25 edges
10. `complete_all_service_tasks()` - 23 edges

## Surprising Connections (you probably didn't know these)
- `parse_bpmn_xml()` --calls--> `restore_from_nats()`  [INFERRED]
  bpmn-parser/src/parser.rs → engine-server/src/main.rs
- `parse_bpmn_xml()` --calls--> `deploy_definition()`  [INFERRED]
  bpmn-parser/src/parser.rs → engine-server/src/server/deploy.rs
- `sleep()` --calls--> `run_consumer()`  [INFERRED]
  bpmn-ninja-external-task-client/src/utils/retry.ts → desktop-tauri/src-tauri/src/sse_consumer.rs
- `sleep()` --calls--> `mutation_fetch_service_task_boundary()`  [INFERRED]
  bpmn-ninja-external-task-client/src/utils/retry.ts → engine-core/src/engine/tests/unit_tests.rs
- `sleep()` --calls--> `timer_catch_event_succeeds()`  [INFERRED]
  bpmn-ninja-external-task-client/src/utils/retry.ts → engine-core/src/engine/tests/unit_tests.rs

## Communities

### Community 0 - "Community 0"
Cohesion: 0.06
Nodes (175): test_setup_boundary_message_event(), test_setup_boundary_no_events(), test_setup_boundary_timer_event(), complete_task_for_node(), compliance_complex_gateway_activation(), compliance_exclusive_gateway_routing(), compliance_parallel_gateway_sync(), create_engine() (+167 more)

### Community 1 - "Community 1"
Cohesion: 0.04
Nodes (38): setup_boundary_events(), WorkflowEngine, evaluate_condition(), WorkflowEngine, ProcessDefinition, WorkflowEngine, Value, execute_complex_gateway() (+30 more)

### Community 2 - "Community 2"
Cohesion: 0.02
Nodes (65): handleDeploy(), handleStart(), confirmDelete(), groupByProcess(), handleDownload(), handleView(), handleSearch(), handleResolve() (+57 more)

### Community 3 - "Community 3"
Cohesion: 0.04
Nodes (74): api_delete(), api_get(), api_post(), api_post_no_body(), api_put(), DefinitionInfo, delete_all_definitions(), delete_definition() (+66 more)

### Community 4 - "Community 4"
Cohesion: 0.04
Nodes (40): parse_rhs(), test_equality(), test_numeric_comparisons(), test_parse_rhs_quoted_strings(), test_truthy(), values_cmp(), values_eq(), arb_instance_state() (+32 more)

### Community 5 - "Community 5"
Cohesion: 0.06
Nodes (61): call_activity_eltern_abschluss_nach_kind(), call_activity_eltern_wartet_auf_kind(), call_activity_kind_ist_unterinstanz(), call_activity_mit_kind_service_task(), call_activity_variablen_propagation(), deploy(), deploy_and_start(), start_server() (+53 more)

### Community 6 - "Community 6"
Cohesion: 0.06
Nodes (32): NatsPersistence, engine_events(), CompletedInstancesQuery, get_completed_instance(), get_instance_history(), get_instance_history_entry(), ServerHistoryQuery, make_completed_instance() (+24 more)

### Community 7 - "Community 7"
Cohesion: 0.05
Nodes (22): start_server_with_nats(), test_completed_instance_appears_in_history_instances(), test_history_instances_filter_by_business_key(), test_history_instances_pagination(), verify_instance_history_is_generated_and_retrieved(), ExternalTaskClient, noopLogger(), randomId() (+14 more)

### Community 8 - "Community 8"
Cohesion: 0.1
Nodes (38): add_listeners(), flatten_subprocess(), parse_bpmn_xml(), parse_iso8601_duration(), parse_multi_instance(), parse_repeating_interval(), parse_timer_definition(), parse_boundary_error_event() (+30 more)

### Community 9 - "Community 9"
Cohesion: 0.08
Nodes (7): list_completed_instances(), restore_from_nats(), execute_job(), NatsPersistence, restore_timer_and_message_catch(), test_restore_timer_and_message_catch(), test_restore_user_and_service_tasks()

### Community 10 - "Community 10"
Cohesion: 0.06
Nodes (34): BpmnBoundaryEvent, BpmnCallActivity, BpmnCompensateEventDefinition, BpmnComplexGateway, BpmnConditionExpression, BpmnDefinitions, BpmnEndEvent, BpmnErrorDef (+26 more)

### Community 11 - "Community 11"
Cohesion: 0.11
Nodes (22): FileReference, ActiveToken, append_audit_log_enforces_limit(), append_audit_log_exactly_at_limit_does_not_trim(), append_audit_log_under_limit(), CompensationRecord, file_variable_names_returns_correct_list(), file_variable_names_returns_empty_when_no_files() (+14 more)

### Community 12 - "Community 12"
Cohesion: 0.1
Nodes (19): BpmnErrorRequest, BucketEntriesQuery, CompletedInstancesQuery, CompleteRequest, CompleteServiceTaskRequest, CorrelateMessageRequest, DeleteDefinitionQuery, DeployRequest (+11 more)

### Community 13 - "Community 13"
Cohesion: 0.12
Nodes (7): injectTauriMock(), loadXmlAndClickElement(), makeCompleted(), test_values_eq_numbers_vs_default(), decodeBase64Text(), generateEmptyBpmn(), generateProcessId()

### Community 14 - "Community 14"
Cohesion: 0.13
Nodes (6): MessageDialog(), MonitoringPage(), ProcessDefinitionPage(), useEngineEvents(), usePolling(), useToast()

### Community 15 - "Community 15"
Cohesion: 0.25
Nodes (6): CustomPropertiesProvider, ExpressionEntry(), getConditionType(), isDefaultFlow(), ScriptBodyEntry(), ScriptLanguageEntry()

### Community 16 - "Community 16"
Cohesion: 0.2
Nodes (2): next_expiry_duration_adds_to_now(), next_expiry_repeating_interval()

### Community 17 - "Community 17"
Cohesion: 0.38
Nodes (9): bpmn_error_routes_to_boundary_event(), complete_nonexistent_task_returns_404(), complete_wrong_worker_returns_conflict(), extend_lock_succeeds(), extend_lock_wrong_worker_returns_conflict(), fail_service_task_decrements_retries(), get_service_tasks_returns_list(), setup_locked_task() (+1 more)

### Community 18 - "Community 18"
Cohesion: 0.36
Nodes (5): execute_script_safe(), run_end_scripts(), run_node_scripts(), ScriptConfig, test_script_config_defaults_and_build()

### Community 19 - "Community 19"
Cohesion: 0.22
Nodes (7): BucketEntry, BucketEntryDetail, BucketInfo, CompletedInstanceQuery, HistoryQuery, StorageInfo, WorkflowPersistence

### Community 20 - "Community 20"
Cohesion: 0.47
Nodes (8): correlate_message_with_no_match_returns_empty(), delete_definition_cascade_removes_instances(), delete_definition_without_cascade_with_instances_returns_409(), delete_instance_returns_204(), deploy_and_start(), get_unknown_instance_returns_404(), process_timers_returns_count(), start_server()

### Community 21 - "Community 21"
Cohesion: 0.29
Nodes (1): ScriptPropertiesProvider

### Community 22 - "Community 22"
Cohesion: 0.29
Nodes (1): ProcessDefinitionBuilder

### Community 23 - "Community 23"
Cohesion: 0.33
Nodes (1): TopicPropertiesProvider

### Community 24 - "Community 24"
Cohesion: 0.33
Nodes (1): CalledElementPropertiesProvider

### Community 25 - "Community 25"
Cohesion: 0.4
Nodes (4): PendingMessageCatch, PendingServiceTask, PendingTimer, PendingUserTask

### Community 26 - "Community 26"
Cohesion: 0.4
Nodes (3): CorrelateMessageRequest, CorrelateMessageResponse, get_pending_messages()

### Community 27 - "Community 27"
Cohesion: 0.5
Nodes (1): SequenceFlow

### Community 28 - "Community 28"
Cohesion: 0.5
Nodes (3): ExecutionListener, ListenerEvent, ScopeEventListener

### Community 29 - "Community 29"
Cohesion: 0.67
Nodes (0): 

### Community 30 - "Community 30"
Cohesion: 0.67
Nodes (0): 

### Community 31 - "Community 31"
Cohesion: 0.67
Nodes (2): ConditionInput, FuzzValue

### Community 32 - "Community 32"
Cohesion: 0.67
Nodes (1): ProcessTimersResponse

### Community 33 - "Community 33"
Cohesion: 1.0
Nodes (0): 

### Community 34 - "Community 34"
Cohesion: 1.0
Nodes (1): main()

### Community 35 - "Community 35"
Cohesion: 1.0
Nodes (0): 

### Community 36 - "Community 36"
Cohesion: 1.0
Nodes (0): 

### Community 37 - "Community 37"
Cohesion: 1.0
Nodes (0): 

### Community 38 - "Community 38"
Cohesion: 1.0
Nodes (0): 

### Community 39 - "Community 39"
Cohesion: 1.0
Nodes (0): 

### Community 40 - "Community 40"
Cohesion: 1.0
Nodes (0): 

### Community 41 - "Community 41"
Cohesion: 1.0
Nodes (0): 

### Community 42 - "Community 42"
Cohesion: 1.0
Nodes (0): 

### Community 43 - "Community 43"
Cohesion: 1.0
Nodes (0): 

### Community 44 - "Community 44"
Cohesion: 1.0
Nodes (1): EngineStats

### Community 45 - "Community 45"
Cohesion: 1.0
Nodes (1): EngineEvent

### Community 46 - "Community 46"
Cohesion: 1.0
Nodes (1): BpmnElement

### Community 47 - "Community 47"
Cohesion: 1.0
Nodes (1): MultiInstanceDef

### Community 48 - "Community 48"
Cohesion: 1.0
Nodes (1): EngineError

### Community 49 - "Community 49"
Cohesion: 1.0
Nodes (1): NatsInfo

### Community 50 - "Community 50"
Cohesion: 1.0
Nodes (0): 

### Community 51 - "Community 51"
Cohesion: 1.0
Nodes (0): 

### Community 52 - "Community 52"
Cohesion: 1.0
Nodes (0): 

### Community 53 - "Community 53"
Cohesion: 1.0
Nodes (0): 

### Community 54 - "Community 54"
Cohesion: 1.0
Nodes (0): 

### Community 55 - "Community 55"
Cohesion: 1.0
Nodes (0): 

### Community 56 - "Community 56"
Cohesion: 1.0
Nodes (0): 

### Community 57 - "Community 57"
Cohesion: 1.0
Nodes (0): 

### Community 58 - "Community 58"
Cohesion: 1.0
Nodes (0): 

### Community 59 - "Community 59"
Cohesion: 1.0
Nodes (0): 

### Community 60 - "Community 60"
Cohesion: 1.0
Nodes (0): 

### Community 61 - "Community 61"
Cohesion: 1.0
Nodes (0): 

### Community 62 - "Community 62"
Cohesion: 1.0
Nodes (0): 

### Community 63 - "Community 63"
Cohesion: 1.0
Nodes (0): 

### Community 64 - "Community 64"
Cohesion: 1.0
Nodes (0): 

### Community 65 - "Community 65"
Cohesion: 1.0
Nodes (0): 

### Community 66 - "Community 66"
Cohesion: 1.0
Nodes (0): 

### Community 67 - "Community 67"
Cohesion: 1.0
Nodes (0): 

### Community 68 - "Community 68"
Cohesion: 1.0
Nodes (0): 

### Community 69 - "Community 69"
Cohesion: 1.0
Nodes (0): 

### Community 70 - "Community 70"
Cohesion: 1.0
Nodes (0): 

### Community 71 - "Community 71"
Cohesion: 1.0
Nodes (0): 

### Community 72 - "Community 72"
Cohesion: 1.0
Nodes (0): 

### Community 73 - "Community 73"
Cohesion: 1.0
Nodes (0): 

### Community 74 - "Community 74"
Cohesion: 1.0
Nodes (0): 

### Community 75 - "Community 75"
Cohesion: 1.0
Nodes (0): 

### Community 76 - "Community 76"
Cohesion: 1.0
Nodes (0): 

### Community 77 - "Community 77"
Cohesion: 1.0
Nodes (0): 

### Community 78 - "Community 78"
Cohesion: 1.0
Nodes (0): 

### Community 79 - "Community 79"
Cohesion: 1.0
Nodes (0): 

### Community 80 - "Community 80"
Cohesion: 1.0
Nodes (0): 

### Community 81 - "Community 81"
Cohesion: 1.0
Nodes (0): 

### Community 82 - "Community 82"
Cohesion: 1.0
Nodes (0): 

### Community 83 - "Community 83"
Cohesion: 1.0
Nodes (0): 

### Community 84 - "Community 84"
Cohesion: 1.0
Nodes (0): 

### Community 85 - "Community 85"
Cohesion: 1.0
Nodes (0): 

### Community 86 - "Community 86"
Cohesion: 1.0
Nodes (0): 

### Community 87 - "Community 87"
Cohesion: 1.0
Nodes (0): 

### Community 88 - "Community 88"
Cohesion: 1.0
Nodes (0): 

### Community 89 - "Community 89"
Cohesion: 1.0
Nodes (0): 

### Community 90 - "Community 90"
Cohesion: 1.0
Nodes (0): 

### Community 91 - "Community 91"
Cohesion: 1.0
Nodes (0): 

### Community 92 - "Community 92"
Cohesion: 1.0
Nodes (0): 

### Community 93 - "Community 93"
Cohesion: 1.0
Nodes (0): 

### Community 94 - "Community 94"
Cohesion: 1.0
Nodes (0): 

### Community 95 - "Community 95"
Cohesion: 1.0
Nodes (0): 

### Community 96 - "Community 96"
Cohesion: 1.0
Nodes (0): 

### Community 97 - "Community 97"
Cohesion: 1.0
Nodes (0): 

### Community 98 - "Community 98"
Cohesion: 1.0
Nodes (0): 

### Community 99 - "Community 99"
Cohesion: 1.0
Nodes (0): 

### Community 100 - "Community 100"
Cohesion: 1.0
Nodes (0): 

### Community 101 - "Community 101"
Cohesion: 1.0
Nodes (0): 

### Community 102 - "Community 102"
Cohesion: 1.0
Nodes (0): 

### Community 103 - "Community 103"
Cohesion: 1.0
Nodes (0): 

### Community 104 - "Community 104"
Cohesion: 1.0
Nodes (0): 

### Community 105 - "Community 105"
Cohesion: 1.0
Nodes (0): 

### Community 106 - "Community 106"
Cohesion: 1.0
Nodes (0): 

### Community 107 - "Community 107"
Cohesion: 1.0
Nodes (0): 

### Community 108 - "Community 108"
Cohesion: 1.0
Nodes (0): 

## Knowledge Gaps
- **120 isolated node(s):** `BpmnMultiInstanceLoopCharacteristics`, `BpmnLoopCardinality`, `BpmnExtensionElements`, `BpmnExecutionListener`, `BpmnScript` (+115 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 33`** (2 nodes): `main()`, `test_cron.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 34`** (2 nodes): `main()`, `build.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 35`** (2 nodes): `LogStream.tsx`, `LogStream()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 36`** (2 nodes): `HistoryTimeline.tsx`, `HistoryTimeline()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 37`** (2 nodes): `EmptyState.tsx`, `EmptyState()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 38`** (2 nodes): `EngineOfflineBanner.tsx`, `EngineOfflineBanner()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 39`** (2 nodes): `use-engine-status.ts`, `useEngineStatus()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 40`** (2 nodes): `Badge()`, `badge.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 41`** (2 nodes): `skeleton.tsx`, `Skeleton()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 42`** (2 nodes): `utils.ts`, `cn()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 43`** (2 nodes): `fix_tests.py`, `fix_tests()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 44`** (2 nodes): `stats.rs`, `EngineStats`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 45`** (2 nodes): `events.rs`, `EngineEvent`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 46`** (2 nodes): `BpmnElement`, `element.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 47`** (2 nodes): `multi_instance.rs`, `MultiInstanceDef`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 48`** (2 nodes): `error.rs`, `EngineError`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 49`** (2 nodes): `NatsInfo`, `models.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 50`** (1 nodes): `lib.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 51`** (1 nodes): `vitest.config.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 52`** (1 nodes): `index.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 53`** (1 nodes): `tailwind.config.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 54`** (1 nodes): `playwright.config.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 55`** (1 nodes): `eslint.config.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 56`** (1 nodes): `vite.config.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 57`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 58`** (1 nodes): `vite-env.d.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 59`** (1 nodes): `InstanceViewer.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 60`** (1 nodes): `engine.ts`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 61`** (1 nodes): `PageHeader.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 62`** (1 nodes): `alert-dialog.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 63`** (1 nodes): `tabs.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 64`** (1 nodes): `card.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 65`** (1 nodes): `toaster.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 66`** (1 nodes): `scroll-area.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 67`** (1 nodes): `label.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 68`** (1 nodes): `accordion.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 69`** (1 nodes): `dialog.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 70`** (1 nodes): `table.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 71`** (1 nodes): `separator.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 72`** (1 nodes): `button.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 73`** (1 nodes): `toast.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 74`** (1 nodes): `select.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 75`** (1 nodes): `textarea.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 76`** (1 nodes): `input.tsx`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 77`** (1 nodes): `fuzz_iso8601_duration.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 78`** (1 nodes): `fuzz_cron_expression.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 79`** (1 nodes): `fuzz_rhai_script.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 80`** (1 nodes): `fuzz_bpmn_parser.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 81`** (1 nodes): `fuzz_deploy_roundtrip.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 82`** (1 nodes): `lib.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 83`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 84`** (1 nodes): `constants.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 85`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 86`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 87`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 88`** (1 nodes): `user_task.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 89`** (1 nodes): `process_start.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 90`** (1 nodes): `definition_ops.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 91`** (1 nodes): `instance_ops.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 92`** (1 nodes): `timer_processor.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 93`** (1 nodes): `persistence_ops.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 94`** (1 nodes): `message_processor.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 95`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 96`** (1 nodes): `completion.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 97`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 98`** (1 nodes): `next_action.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 99`** (1 nodes): `parallel.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 100`** (1 nodes): `events.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 101`** (1 nodes): `tasks.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 102`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 103`** (1 nodes): `sub_processes.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 104`** (1 nodes): `gateways.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 105`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 106`** (1 nodes): `trait_impl.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 107`** (1 nodes): `lib.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 108`** (1 nodes): `lib.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `groupByProcess()` connect `Community 2` to `Community 1`?**
  _High betweenness centrality (0.038) - this node is a cross-community bridge._
- **Why does `parse_bpmn_xml()` connect `Community 8` to `Community 0`, `Community 1`, `Community 9`?**
  _High betweenness centrality (0.037) - this node is a cross-community bridge._
- **Are the 133 inferred relationships involving `deploy_definition()` (e.g. with `api_post()` and `compliance_exclusive_gateway_routing()`) actually correct?**
  _`deploy_definition()` has 133 INFERRED edges - model-reasoned connections that need verification._
- **Are the 106 inferred relationships involving `start_instance()` (e.g. with `.is_empty()` and `api_post()`) actually correct?**
  _`start_instance()` has 106 INFERRED edges - model-reasoned connections that need verification._
- **Are the 41 inferred relationships involving `parse_bpmn_xml()` (e.g. with `parse_simple_bpmn()` and `parse_conditional_flows()`) actually correct?**
  _`parse_bpmn_xml()` has 41 INFERRED edges - model-reasoned connections that need verification._
- **What connects `BpmnMultiInstanceLoopCharacteristics`, `BpmnLoopCardinality`, `BpmnExtensionElements` to the rest of the system?**
  _120 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.06 - nodes in this community are weakly interconnected._
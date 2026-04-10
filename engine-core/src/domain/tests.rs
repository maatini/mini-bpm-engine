use super::*;
use uuid::Uuid;

    #[test]
    fn valid_definition_with_builder() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node(
                "svc",
                BpmnElement::ServiceTask {
                    topic: "do_it".into(),
                    multi_instance: None,
                },
            )
            .node("end", BpmnElement::EndEvent)
            .flow("start", "svc")
            .flow("svc", "end")
            .build();
        assert!(def.is_ok());
    }

    #[test]
    fn rejects_missing_start() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("end", BpmnElement::EndEvent)
            .build();
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("No start event")
        ));
    }

    #[test]
    fn rejects_missing_end() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .flow("start", "nowhere")
            .build();
        assert!(def.is_err());
    }

    #[test]
    fn rejects_dangling_flow() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node("end", BpmnElement::EndEvent)
            .flow("start", "end")
            .flow("end", "ghost")
            .build();
        assert!(matches!(def, Err(EngineError::NoSuchNode(id)) if id == "ghost"));
    }

    #[test]
    fn rejects_node_without_outgoing_flow() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node(
                "orphan",
                BpmnElement::ServiceTask {
                    topic: "noop".into(),
                    multi_instance: None,
                },
            )
            .node("end", BpmnElement::EndEvent)
            .flow("start", "end")
            .build();
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("orphan")
        ));
    }

    #[test]
    fn find_node_and_next_work() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node(
                "svc",
                BpmnElement::ServiceTask {
                    topic: "action".into(),
                    multi_instance: None,
                },
            )
            .node("end", BpmnElement::EndEvent)
            .flow("start", "svc")
            .flow("svc", "end")
            .build()
            .unwrap();

        assert_eq!(
            def.get_node("svc"),
            Some(&BpmnElement::ServiceTask {
                topic: "action".into(),
                multi_instance: None
            })
        );
        assert_eq!(def.next_node("start"), Some("svc"));
        assert_eq!(def.next_node("end"), None);
    }

    #[test]
    fn token_creation() {
        let token = Token::new("start");
        assert_eq!(token.current_node, "start");
        assert!(token.variables.is_empty());
    }

    #[test]
    fn token_is_merged_survives_serialization() {
        let mut token = Token::new("gw_join");
        token.is_merged = true;
        let json = serde_json::to_string(&token).unwrap();
        let restored: Token = serde_json::from_str(&json).unwrap();
        assert!(restored.is_merged, "is_merged must survive roundtrip");
    }

    #[test]
    fn timer_start_event_definition() {
        let def = ProcessDefinitionBuilder::new("timer")
            .node(
                "ts",
                BpmnElement::TimerStartEvent(crate::domain::TimerDefinition::Duration(
                    std::time::Duration::from_secs(5),
                )),
            )
            .node("end", BpmnElement::EndEvent)
            .flow("ts", "end")
            .build();
        assert!(def.is_ok());
    }

    // --- Gateway-specific tests ---

    #[test]
    fn exclusive_gateway_definition() {
        let def = ProcessDefinitionBuilder::new("xor")
            .node("start", BpmnElement::StartEvent)
            .node(
                "gw",
                BpmnElement::ExclusiveGateway {
                    default: Some("end2".into()),
                },
            )
            .node("end1", BpmnElement::EndEvent)
            .node("end2", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "end1", "approved == true")
            .flow("gw", "end2")
            .build();
        assert!(def.is_ok());
    }

    #[test]
    fn inclusive_gateway_definition() {
        let def = ProcessDefinitionBuilder::new("or")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::InclusiveGateway)
            .node("end1", BpmnElement::EndEvent)
            .node("end2", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "end1", "notify_email == true")
            .conditional_flow("gw", "end2", "notify_sms == true")
            .build();
        assert!(def.is_ok());
    }

    #[test]
    fn gateway_rejects_single_outgoing() {
        let def = ProcessDefinitionBuilder::new("bad")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: None })
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .flow("gw", "end")
            .build();
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("at least 2")
        ));
    }

    #[test]
    fn event_based_gateway_rejects_non_catch_targets() {
        let def = ProcessDefinitionBuilder::new("bad_gw")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::EventBasedGateway)
            .node(
                "task",
                BpmnElement::ServiceTask {
                    topic: "noop".into(),
                    multi_instance: None,
                },
            )
            .node(
                "catch",
                BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                    std::time::Duration::from_secs(5),
                )),
            )
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .flow("gw", "task")
            .flow("gw", "catch")
            .flow("task", "end")
            .flow("catch", "end")
            .build();

        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("EventBasedGateway") && msg.contains("can only connect to MessageCatchEvent or TimerCatchEvent")
        ));
    }

    #[test]
    fn conditional_flow_builder() {
        let def = ProcessDefinitionBuilder::new("cond")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: None })
            .node("a", BpmnElement::EndEvent)
            .node("b", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "a", "x == 1")
            .conditional_flow("gw", "b", "x == 2")
            .build()
            .unwrap();

        let flows = def.next_nodes("gw");
        assert_eq!(flows.len(), 2);
        assert_eq!(flows[0].condition, Some("x == 1".into()));
        assert_eq!(flows[1].condition, Some("x == 2".into()));
    }

    #[test]
    fn next_nodes_returns_multiple() {
        let def = ProcessDefinitionBuilder::new("multi")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::InclusiveGateway)
            .node("a", BpmnElement::EndEvent)
            .node("b", BpmnElement::EndEvent)
            .node("c", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "a", "x > 0")
            .conditional_flow("gw", "b", "y > 0")
            .conditional_flow("gw", "c", "z > 0")
            .build()
            .unwrap();

        assert_eq!(def.next_nodes("gw").len(), 3);
        // next_node returns None for multi-out nodes
        assert_eq!(def.next_node("gw"), None);
    }

    #[test]
    fn test_file_reference_roundtrip() {
        let instance_id = Uuid::new_v4();
        let file_ref = FileReference::new(
            instance_id,
            "contract",
            "contract.pdf",
            "application/pdf",
            1024 * 500,
        );

        let value = file_ref.to_variable_value();

        assert_eq!(value.get("type").unwrap().as_str().unwrap(), "file");
        assert_eq!(
            value.get("filename").unwrap().as_str().unwrap(),
            "contract.pdf"
        );

        let restored = FileReference::from_variable_value(&value).unwrap();
        assert_eq!(file_ref, restored);
    }

    #[test]
    fn test_is_join_gateway() {
        // Catches: replace is_join_gateway -> bool with true
        // Use a definition where the gateway has 2 outgoing but 1 incoming
        let def = ProcessDefinitionBuilder::new("join")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: None })
            .node("end_a", BpmnElement::EndEvent)
            .node("end_b", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "end_a", "x == 1")
            .conditional_flow("gw", "end_b", "x == 2")
            .build()
            .unwrap();
        // gw has 1 incoming → not a join
        assert!(!def.is_join_gateway("gw"));
    }

    #[test]
    fn test_is_join_gateway_with_two_incoming() {
        let def = ProcessDefinitionBuilder::new("join2")
            .node("start", BpmnElement::StartEvent)
            .node("a", BpmnElement::ScriptTask { script: "let x = 1;".into(), multi_instance: None })
            .node("b", BpmnElement::ScriptTask { script: "let y = 1;".into(), multi_instance: None })
            .node("split", BpmnElement::ParallelGateway)
            .node("join", BpmnElement::ParallelGateway)
            .node("end", BpmnElement::EndEvent)
            .flow("start", "split")
            .flow("split", "a")
            .flow("split", "b")
            .flow("a", "join")
            .flow("b", "join")
            .flow("join", "end")
            .build()
            .unwrap();
        assert!(def.is_join_gateway("join"));
        assert!(!def.is_join_gateway("split"));
    }

    #[test]
    fn test_is_split_gateway() {
        // Catches: replace is_split_gateway -> bool with true/false,
        //          replace && with ||, replace >= with <
        let def = ProcessDefinitionBuilder::new("split")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: None })
            .node("end_a", BpmnElement::EndEvent)
            .node("end_b", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "end_a", "x == 1")
            .conditional_flow("gw", "end_b", "x == 2")
            .build()
            .unwrap();
        // 2 outgoing + is gateway type → split
        assert!(def.is_split_gateway("gw"));
        // Non-gateway nodes are never split gateways
        assert!(!def.is_split_gateway("start"));
        // Non-existent node
        assert!(!def.is_split_gateway("nonexistent"));
    }

    #[test]
    fn test_is_split_gateway_single_outgoing() {
        // Build a definition where a gateway acts as join-only (2 incoming, 1 outgoing)
        // Use the join from test_is_join_gateway_with_two_incoming
        let def = ProcessDefinitionBuilder::new("join_only")
            .node("start", BpmnElement::StartEvent)
            .node("a", BpmnElement::ScriptTask { script: "let x = 1;".into(), multi_instance: None })
            .node("b", BpmnElement::ScriptTask { script: "let y = 1;".into(), multi_instance: None })
            .node("split", BpmnElement::ParallelGateway)
            .node("join", BpmnElement::ParallelGateway)
            .node("end", BpmnElement::EndEvent)
            .flow("start", "split")
            .flow("split", "a")
            .flow("split", "b")
            .flow("a", "join")
            .flow("b", "join")
            .flow("join", "end")
            .build()
            .unwrap();
        // join has 2 incoming, 1 outgoing → not a split
        assert!(!def.is_split_gateway("join"));
        // split has 1 incoming, 2 outgoing → is a split
        assert!(def.is_split_gateway("split"));
    }

    #[test]
    fn test_get_file_reference_returns_none_for_non_file() {
        let not_a_file = serde_json::json!({
            "type": "string",
            "value": "hello"
        });

        let result = FileReference::from_variable_value(&not_a_file);
        assert!(result.is_none());

        // Also just a string
        let just_a_str = serde_json::json!("file");
        assert!(FileReference::from_variable_value(&just_a_str).is_none());
    }

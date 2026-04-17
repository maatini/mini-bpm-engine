//! Fuzz target: End-to-end deploy + start roundtrip.
//!
//! Tests the full pipeline: arbitrary XML → parse → deploy → start instance.
//! This catches logic errors that isolated parser or engine fuzzers miss,
//! such as panics during token execution on weird graph topologies.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only process reasonable-length, valid UTF-8 strings
    let xml = match std::str::from_utf8(data) {
        Ok(s) if s.len() >= 10 && s.len() <= 65536 => s,
        _ => return,
    };

    // Step 1: Parse — must not panic (errors are fine)
    let definition = match bpmn_parser::parse_bpmn_xml(xml) {
        Ok(def) => def,
        Err(_) => return, // Invalid XML is expected; just don't panic
    };

    // Step 2: Deploy + Start — exercise the engine in-memory
    // Use a dedicated tokio runtime per fuzzing iteration to avoid state leakage
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };

    rt.block_on(async {
        let engine = engine_core::engine::WorkflowEngine::new()
            .with_persistence(std::sync::Arc::new(persistence_memory::InMemoryPersistence::new()));

        // Deploy — must not panic even on pathological definitions
        let (def_key, _version) = engine.deploy_definition(definition).await;

        // Start instance — must not panic
        let start_result = engine.start_instance(def_key).await;

        // If start succeeded, try interacting with the instance
        if let Ok(instance_id) = start_result {
            // Try to list tasks — must not panic
            let _ = engine.get_pending_user_tasks();
            let _ = engine.get_pending_service_tasks();

            // Try to get instance details — must not panic
            let _ = engine.get_instance_details(instance_id).await;
        }
    });
});

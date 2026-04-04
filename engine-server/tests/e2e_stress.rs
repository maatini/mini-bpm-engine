use serde_json::Value;

async fn start_server() -> String {
    let app = engine_server::build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind to random port");
    let addr = listener.local_addr().expect("failed to get local addr");
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server error");
    });

    base_url
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_http_concurrent_deployments() {
    let base = start_server().await;
    let client = reqwest::Client::builder().pool_max_idle_per_host(50).build().unwrap();
    
    let mut handles = Vec::new();
    
    for i in 0..20 {
        let cli = client.clone();
        let b = base.clone();
        handles.push(tokio::spawn(async move {
            let xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
                <bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" id="Definitions_{i}" targetNamespace="http://bpmn.io/schema/bpmn">
              <bpmn:process id="Process_stress_{i}" isExecutable="true">
                <bpmn:startEvent id="StartEvent_1">
                  <bpmn:outgoing>Flow_1</bpmn:outgoing>
                </bpmn:startEvent>
                <bpmn:endEvent id="EndEvent_1">
                  <bpmn:incoming>Flow_1</bpmn:incoming>
                </bpmn:endEvent>
                <bpmn:sequenceFlow id="Flow_1" sourceRef="StartEvent_1" targetRef="EndEvent_1"/>
              </bpmn:process>
                </bpmn:definitions>
            "#);
            
            let res = cli.post(format!("{}/api/deploy", b))
                .json(&serde_json::json!({ "xml": xml, "name": format!("p_{}", i) }))
                .send()
                .await
                .expect("request failed");
            
            let status = res.status();
            if !status.is_success() {
                let text = res.text().await.unwrap_or_default();
                println!("Deployment failed: {} {}", status, text);
            }
            status
        }));
    }
    
    let mut successes = 0;
    for h in handles {
        if h.await.unwrap() == 200 {
            successes += 1;
        }
    }
    assert_eq!(successes, 20);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_http_concurrent_starts() {
    let base = start_server().await;
    let client = reqwest::Client::builder().pool_max_idle_per_host(100).build().unwrap();
    
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
          <bpmn:process id="Process_stress_start" isExecutable="true">
            <bpmn:startEvent id="StartEvent_1">
              <bpmn:outgoing>Flow_1</bpmn:outgoing>
            </bpmn:startEvent>
            <bpmn:endEvent id="EndEvent_1">
              <bpmn:incoming>Flow_1</bpmn:incoming>
            </bpmn:endEvent>
            <bpmn:sequenceFlow id="Flow_1" sourceRef="StartEvent_1" targetRef="EndEvent_1"/>
          </bpmn:process>
        </bpmn:definitions>
    "#;
    
    let deploy_res = client.post(format!("{}/api/deploy", base))
        .json(&serde_json::json!({ "xml": xml, "name": "stress_start" }))
        .send()
        .await
        .expect("deploy failed");
    
    let body: Value = deploy_res.json().await.unwrap();
    let def_key = body["definition_key"].as_str().unwrap().to_string();
    
    let mut handles = Vec::new();
    
    for _ in 0..100 {
        let cli = client.clone();
        let b = base.clone();
        let key = def_key.clone();
        handles.push(tokio::spawn(async move {
            let res = cli.post(format!("{}/api/start", b))
                .json(&serde_json::json!({ "definition_key": key, "variables": {} }))
                .send()
                .await
                .expect("start request failed");
            res.status()
        }));
    }
    
    let mut successes = 0;
    for h in handles {
        if h.await.unwrap() == 200 {
            successes += 1;
        }
    }
    assert_eq!(successes, 100);
    
    // Validate stats
    let stats_res = client.get(format!("{}/api/monitoring", base))
        .send().await.unwrap();
    let stats: Value = stats_res.json().await.unwrap();
    let completed = stats["instances_completed"].as_u64().unwrap();
    assert!(completed >= 100);
}

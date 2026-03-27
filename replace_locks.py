import re

def update_file(path):
    with open(path, 'r') as f:
        content = f.read()

    # Replace AppState Mutex with RwLock
    content = content.replace('tokio::sync::Mutex', 'tokio::sync::RwLock')
    content = content.replace('engine: Arc<Mutex<WorkflowEngine>>', 'engine: Arc<RwLock<WorkflowEngine>>')
    content = content.replace('deployed_xml: Arc<Mutex<HashMap<String, String>>>', 'deployed_xml: Arc<RwLock<HashMap<String, String>>>')
    content = content.replace('Arc::new(Mutex::new(xml_cache))', 'Arc::new(RwLock::new(xml_cache))')
    content = content.replace('Arc::new(Mutex::new(WorkflowEngine::new()))', 'Arc::new(RwLock::new(WorkflowEngine::new()))')
    content = content.replace('engine: Arc<Mutex<WorkflowEngine>>', 'engine: Arc<RwLock<WorkflowEngine>>')
    
    # Replace .lock().await with .read().await or .write().await
    # if it says "let mut engine = ... lock().await;", it should be write().await
    # if it says "let engine = ... lock().await;", it should be read().await
    content = re.sub(r'let mut engine\s*=\s*(.*?)\.lock\(\)\.await;', r'let mut engine = \1.write().await;', content)
    content = re.sub(r'let engine\s*=\s*(.*?)\.lock\(\)\.await;', r'let engine = \1.read().await;', content)
    
    # Also deployed_xml
    content = re.sub(r'state\.deployed_xml\.lock\(\)\.await\.insert', r'state.deployed_xml.write().await.insert', content)
    content = re.sub(r'state\.deployed_xml\.lock\(\)\.await\.remove', r'state.deployed_xml.write().await.remove', content)
    content = re.sub(r'let xml_store\s*=\s*state\.deployed_xml\.lock\(\)\.await;', r'let xml_store = state.deployed_xml.read().await;', content)

    with open(path, 'w') as f:
        f.write(content)

update_file('engine-server/src/server.rs')
update_file('engine-server/src/main.rs')

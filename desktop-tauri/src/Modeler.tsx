import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/api/dialog';
import { readBpmnFile, uploadInstanceFile } from './lib/tauri';
import { FilePlus, FolderOpen, UploadCloud, Play, Focus } from 'lucide-react';

// Make sure to ignore TS types for modules that might not have types
// @ts-ignore
import BpmnModeler from 'bpmn-js/lib/Modeler';
// @ts-ignore
import { BpmnPropertiesPanelModule, BpmnPropertiesProviderModule } from 'bpmn-js-properties-panel';
// @ts-ignore
import camundaModdleDescriptor from 'camunda-bpmn-moddle/resources/camunda.json';

import 'bpmn-js/dist/assets/diagram-js.css';
import 'bpmn-js/dist/assets/bpmn-font/css/bpmn-embedded.css';
import '@bpmn-io/properties-panel/assets/properties-panel.css';

import { CustomPropertiesProvider } from './ConditionPropertiesProvider';
import { ScriptPropertiesProvider } from './ScriptPropertiesProvider';
import { TopicPropertiesProvider } from './TopicPropertiesProvider';
import { VariableEditor, type VariableRow, serializeVariables } from './VariableEditor';

const customProviderModule = {
  __init__: ['customPropertiesProvider', 'scriptPropertiesProvider', 'topicPropertiesProvider'],
  customPropertiesProvider: ['type', CustomPropertiesProvider],
  scriptPropertiesProvider: ['type', ScriptPropertiesProvider],
  topicPropertiesProvider: ['type', TopicPropertiesProvider]
};


interface ModelerProps {
  onDeploy: (xml: string) => Promise<void>;
  onStart: (xml: string, variables: Record<string, unknown>) => Promise<string>;
  onNewDiagram: () => void;
  onOpenFile: () => void;
  initialXml?: string | null;
}

// Generates a unique process ID like "process-a1b2c3d4"
function generateProcessId(): string {
  const hex = Array.from(crypto.getRandomValues(new Uint8Array(4)))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
  return `process-${hex}`;
}

// Default empty BPMN diagram shown when no definition is loaded
function generateEmptyBpmn(): string {
  const pid = generateProcessId();
  return `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="${pid}" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="${pid}">
      <bpmndi:BPMNShape id="_BPMNShape_StartEvent_2" bpmnElement="StartEvent_1">
        <dc:Bounds x="150" y="100" width="36" height="36" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;
}

export function Modeler({ onDeploy, onStart, onNewDiagram, onOpenFile, initialXml }: ModelerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const propertiesRef = useRef<HTMLDivElement>(null);
  const modelerRef = useRef<any>(null);
  // Track the last imported XML to avoid redundant re-imports
  const lastImportedXmlRef = useRef<string | null>(null);
  const [showVarsDialog, setShowVarsDialog] = useState(false);
  const [businessKey, setBusinessKey] = useState('');
  const [startVariables, setStartVariables] = useState<VariableRow[]>([]);
  const [isStarting, setIsStarting] = useState(false);

  useEffect(() => {
    if (containerRef.current && propertiesRef.current) {
      if (modelerRef.current) return;
      
      const modeler: any = new BpmnModeler({
        container: containerRef.current,
        propertiesPanel: { parent: propertiesRef.current },
        additionalModules: [
          BpmnPropertiesPanelModule,
          BpmnPropertiesProviderModule,
          customProviderModule
        ],
        moddleExtensions: {
          camunda: camundaModdleDescriptor
        }
      });
      
      modelerRef.current = modeler;
      // Import the initial XML or a saved diagram, or a default empty diagram
      const savedXml = localStorage.getItem('minibpm_last_workflow');
      const xmlToLoad = initialXml || savedXml || generateEmptyBpmn();
      modeler.importXML(xmlToLoad);
      lastImportedXmlRef.current = initialXml || null;
      
      modeler.on('commandStack.changed', async () => {
        try {
          const { xml } = await modeler.saveXML({ format: true });
          if (xml) localStorage.setItem('minibpm_last_workflow', xml);
        } catch { }
      });
      
      modeler.on('import.done', async ({ error }: any) => {
        if (!error) {
          modeler.get('canvas').zoom('fit-viewport', 'auto');
          try {
            const { xml } = await modeler.saveXML({ format: true });
            if (xml) localStorage.setItem('minibpm_last_workflow', xml);
          } catch { }
        }
      });

      
      return () => { 
        if (modelerRef.current) {
          modeler.destroy(); 
          modelerRef.current = null; 
        }
      };
    }
  }, []);

  // Re-import when initialXml changes (e.g. user clicks "View in Modeler")
  useEffect(() => {
    if (modelerRef.current && initialXml && initialXml !== lastImportedXmlRef.current) {
      modelerRef.current.importXML(initialXml);
      lastImportedXmlRef.current = initialXml;
    }
  }, [initialXml]);

  const handleNewDiagram = async () => {
    if (!modelerRef.current) return;
    try {
      await modelerRef.current.importXML(generateEmptyBpmn());
      lastImportedXmlRef.current = null;
      onNewDiagram();
    } catch (e: any) {
      console.error("Failed to create new diagram", e);
    }
  };

  const handleOpenFile = async () => {
    try {
      const selected = await open({
        filters: [{ name: 'BPMN', extensions: ['bpmn', 'xml'] }],
        multiple: false,
      });
      if (!selected || Array.isArray(selected)) return;
      const xml = await readBpmnFile(selected);
      if (modelerRef.current) {
        await modelerRef.current.importXML(xml);
        lastImportedXmlRef.current = xml;
      }
      onOpenFile();
    } catch (e: any) {
      alert('Failed to open BPMN file: ' + e);
    }
  };

  const handleDeploy = async () => {
    if (!modelerRef.current) return;
    try {
      const { xml } = await modelerRef.current.saveXML({ format: true });
      await onDeploy(xml);
    } catch (e: any) {
      console.error("Failed to save XML", e);
    }
  };

  const handleCenter = () => {
    if (modelerRef.current) {
      modelerRef.current.get('canvas').zoom('fit-viewport', 'auto');
    }
  };

  const handleStartClick = () => {
    setStartVariables([]);
    setBusinessKey('');
    setShowVarsDialog(true);
  };

  const handleStartConfirm = async () => {
    if (!modelerRef.current) return;

    const serialized = serializeVariables(startVariables);
    if (serialized === null) return;

    if (businessKey.trim() !== '') {
      serialized.business_key = businessKey.trim();
    }

    // Collect pending file rows for deferred upload
    const pendingFiles = startVariables.filter(v => v.type === 'File' && v.pendingFilePath);

    setIsStarting(true);
    try {
      const { xml } = await modelerRef.current.saveXML({ format: true });
      setShowVarsDialog(false);
      const instanceId = await onStart(xml, serialized);

      // Upload pending files after instance creation
      for (const pf of pendingFiles) {
        if (pf.pendingFilePath && pf.name.trim()) {
          try {
            await uploadInstanceFile(instanceId, pf.name.trim(), pf.pendingFilePath);
          } catch (uploadErr) {
            console.error(`Failed to upload file '${pf.name}':`, uploadErr);
          }
        }
      }
    } catch (e: any) {
      alert('Failed to start process: ' + e);
    } finally {
      setIsStarting(false);
    }
  };

  return (
    <>
      <div className="header-actions">
        <button className="button" onClick={handleNewDiagram} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><FilePlus size={16} /> New Diagram</button>
        <button className="button" onClick={handleOpenFile} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><FolderOpen size={16} /> Open File</button>
        <button className="button" onClick={handleDeploy} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}><UploadCloud size={16} /> Deploy Process</button>
        <button className="button" onClick={handleStartClick} style={{ backgroundColor: '#10b981', display: 'flex', alignItems: 'center', gap: '6px' }}><Play size={16} /> Start Instance</button>
      </div>
      <div className="modeler-container" style={{ position: 'relative' }}>
        <div className="canvas" ref={containerRef} />
        <button 
          onClick={handleCenter}
          style={{ position: 'absolute', bottom: '48px', right: '316px', zIndex: 99, padding: '6px 8px', backgroundColor: 'white', border: '1px solid #cbd5e1', borderRadius: '4px', cursor: 'pointer', boxShadow: '0 1px 3px rgba(0,0,0,0.1)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          title="Center Workflow"
        >
          <Focus size={18} color="#475569" />
        </button>
        <div className="properties-panel-parent" ref={propertiesRef} />
      </div>

      {showVarsDialog && (
        <div className="vars-dialog-overlay" onClick={() => setShowVarsDialog(false)}>
          <div className="vars-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>Start Process Instance</h3>
            
            <label style={{ display: 'block', marginBottom: '4px', fontSize: '0.9rem', color: '#475569', fontWeight: 500 }}>
              Business Key (optional)
            </label>
            <input
              type="text"
              value={businessKey}
              onChange={(e) => setBusinessKey(e.target.value)}
              placeholder="e.g. ORDER-1000"
              style={{ width: '100%', padding: '8px', marginBottom: '16px', border: '1px solid #cbd5e1', borderRadius: '4px', fontFamily: 'inherit', fontSize: '0.9rem' }}
            />

            <label style={{ display: 'block', marginBottom: '4px', fontSize: '0.9rem', color: '#475569', fontWeight: 500 }}>
              Process Variables
            </label>
            <VariableEditor
              variables={startVariables}
              onChange={setStartVariables}
              allowPendingFiles={true}
            />
            <div className="vars-dialog-actions">
              <button className="button" onClick={() => setShowVarsDialog(false)} style={{backgroundColor: '#6b7280'}}>Cancel</button>
              <button className="button" onClick={handleStartConfirm} style={{backgroundColor: '#10b981'}} disabled={isStarting}>
                {isStarting ? 'Deploying & Starting…' : 'Start'}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

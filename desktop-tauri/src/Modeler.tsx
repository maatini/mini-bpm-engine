import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/api/dialog';
import { readBpmnFile } from './lib/tauri';
import { FilePlus, FolderOpen, UploadCloud, Play } from 'lucide-react';

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

const customProviderModule = {
  __init__: ['customPropertiesProvider', 'scriptPropertiesProvider'],
  customPropertiesProvider: ['type', CustomPropertiesProvider],
  scriptPropertiesProvider: ['type', ScriptPropertiesProvider]
};


interface ModelerProps {
  onDeploy: (xml: string) => Promise<void>;
  onStart: (variables: Record<string, unknown>) => void;
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
  const [varsJson, setVarsJson] = useState('{}');

  useEffect(() => {
    if (containerRef.current && propertiesRef.current) {
      if (modelerRef.current) return;
      
      const modeler = new BpmnModeler({
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
        } catch (e) { }
      });
      
      modeler.on('import.done', async ({ error }: any) => {
        if (!error) {
          try {
            const { xml } = await modeler.saveXML({ format: true });
            if (xml) localStorage.setItem('minibpm_last_workflow', xml);
          } catch (e) { }
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
    } catch (e) {
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
    } catch (e) {
      alert('Failed to open BPMN file: ' + e);
    }
  };

  const handleDeploy = async () => {
    if (!modelerRef.current) return;
    try {
      const { xml } = await modelerRef.current.saveXML({ format: true });
      await onDeploy(xml);
    } catch (e) {
      console.error("Failed to save XML", e);
    }
  };

  const handleStartClick = () => {
    setVarsJson('{}');
    setBusinessKey('');
    setShowVarsDialog(true);
  };

  const handleStartConfirm = () => {
    try {
      const parsed = JSON.parse(varsJson);
      if (typeof parsed !== 'object' || Array.isArray(parsed)) {
        alert('Variables must be a JSON object (e.g. {"key": "value"}).');
        return;
      }
      
      if (businessKey.trim() !== '') {
        parsed.business_key = businessKey.trim();
      }
      
      setShowVarsDialog(false);
      onStart(parsed);
    } catch {
      alert('Invalid JSON. Please enter a valid JSON object.');
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
      <div className="modeler-container">
        <div className="canvas" ref={containerRef} />
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
            <p style={{fontSize: '0.85rem', color: '#888', margin: '0 0 8px'}}>Optional JSON object, e.g. {'{"orderId": "123"}'}</p>
            <textarea
              className="vars-textarea"
              value={varsJson}
              onChange={(e) => setVarsJson(e.target.value)}
              rows={6}
              spellCheck={false}
            />
            <div className="vars-dialog-actions">
              <button className="button" onClick={() => setShowVarsDialog(false)} style={{backgroundColor: '#6b7280'}}>Cancel</button>
              <button className="button" onClick={handleStartConfirm} style={{backgroundColor: '#10b981'}}>Start</button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

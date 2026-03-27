import { useEffect, useRef, useState } from 'react';

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
  initialXml?: string | null;
}

// Default empty BPMN diagram shown when no definition is loaded
const EMPTY_BPMN = `<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" id="Definitions_1" targetNamespace="http://bpmn.io/schema/bpmn">
  <bpmn:process id="Process_1" isExecutable="true">
    <bpmn:startEvent id="StartEvent_1"/>
  </bpmn:process>
  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="Process_1">
      <bpmndi:BPMNShape id="_BPMNShape_StartEvent_2" bpmnElement="StartEvent_1">
        <dc:Bounds x="150" y="100" width="36" height="36" />
      </bpmndi:BPMNShape>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>`;

export function Modeler({ onDeploy, onStart, onNewDiagram, initialXml }: ModelerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const propertiesRef = useRef<HTMLDivElement>(null);
  const modelerRef = useRef<any>(null);
  // Track the last imported XML to avoid redundant re-imports
  const lastImportedXmlRef = useRef<string | null>(null);
  const [showVarsDialog, setShowVarsDialog] = useState(false);
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
      // Import the initial XML or a default empty diagram
      const xmlToLoad = initialXml || EMPTY_BPMN;
      modeler.importXML(xmlToLoad);
      lastImportedXmlRef.current = initialXml || null;
      
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
      await modelerRef.current.importXML(EMPTY_BPMN);
      lastImportedXmlRef.current = null;
      onNewDiagram();
    } catch (e) {
      console.error("Failed to create new diagram", e);
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
    setShowVarsDialog(true);
  };

  const handleStartConfirm = () => {
    try {
      const parsed = JSON.parse(varsJson);
      if (typeof parsed !== 'object' || Array.isArray(parsed)) {
        alert('Variables must be a JSON object (e.g. {"key": "value"}).');
        return;
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
        <button className="button" onClick={handleNewDiagram}>New Diagram</button>
        <button className="button" onClick={handleDeploy}>Deploy Process</button>
        <button className="button" onClick={handleStartClick} style={{backgroundColor: '#10b981'}}>Start Instance</button>
      </div>
      <div className="modeler-container">
        <div className="canvas" ref={containerRef} />
        <div className="properties-panel-parent" ref={propertiesRef} />
      </div>

      {showVarsDialog && (
        <div className="vars-dialog-overlay" onClick={() => setShowVarsDialog(false)}>
          <div className="vars-dialog" onClick={(e) => e.stopPropagation()}>
            <h3>Process Variables</h3>
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

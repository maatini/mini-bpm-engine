import { useEffect, useRef, useState, memo } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { readBpmnFile } from '../../shared/lib/tauri';
import { FilePlus, FolderOpen, UploadCloud, Play, Focus } from 'lucide-react';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { StartInstanceDialog } from './StartInstanceDialog';
import { type VariableRow } from '../../shared/components/VariableEditor';

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

import { CustomPropertiesProvider } from './properties/ConditionPropertiesProvider';
import { ScriptPropertiesProvider } from './properties/ScriptPropertiesProvider';
import { TopicPropertiesProvider } from './properties/TopicPropertiesProvider';
import { CalledElementPropertiesProvider } from './properties/CalledElementPropertiesProvider';

const customProviderModule = {
  __init__: ['customPropertiesProvider', 'scriptPropertiesProvider', 'topicPropertiesProvider', 'calledElementPropertiesProvider'],
  customPropertiesProvider: ['type', CustomPropertiesProvider],
  scriptPropertiesProvider: ['type', ScriptPropertiesProvider],
  topicPropertiesProvider: ['type', TopicPropertiesProvider],
  calledElementPropertiesProvider: ['type', CalledElementPropertiesProvider]
};

interface ModelerPageProps {
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

export const ModelerPage = memo(function ModelerPage({ onDeploy, onStart, onNewDiagram, onOpenFile, initialXml }: ModelerPageProps) {
  const { toast } = useToast();
  const containerRef = useRef<HTMLDivElement>(null);
  const propertiesRef = useRef<HTMLDivElement>(null);
  const modelerRef = useRef<any>(null);
  // Track the last imported XML to avoid redundant re-imports
  const lastImportedXmlRef = useRef<string | null>(null);
  const [showVarsDialog, setShowVarsDialog] = useState(false);

  useEffect(() => {
    let didCancel = false;

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
      // Expose modeler instance for E2E tests
      if (import.meta.env.DEV) {
        (window as any).__bpmnModeler__ = modeler;
      }
      // Import the initial XML or a saved diagram, or a default empty diagram
      const savedXml = localStorage.getItem('minibpm_last_workflow');
      const xmlToLoad = initialXml || savedXml || generateEmptyBpmn();
      
      modeler.importXML(xmlToLoad).then(() => {
        if (!didCancel) {
          lastImportedXmlRef.current = initialXml || null;
        }
      }).catch((err: any) => {
        if (!didCancel) {
          console.error('Failed to import initial XML, falling back to empty', err);
          localStorage.removeItem('minibpm_last_workflow');
          modeler.importXML(generateEmptyBpmn()).catch((fallbackErr: any) => {
            if (!didCancel) console.error('Fallback also failed', fallbackErr);
          });
        }
      });
      
      modeler.on('commandStack.changed', async () => {
        if (didCancel) return;
        try {
          const { xml } = await modeler.saveXML({ format: true });
          if (xml && !didCancel) localStorage.setItem('minibpm_last_workflow', xml);
        } catch (e) {
          if (!didCancel) console.error('Failed to save XML', e);
        }
      });
      
      modeler.on('import.done', async ({ error }: any) => {
        if (didCancel) return;
        if (!error) {
          try {
            modeler.get('canvas').zoom('fit-viewport', 'auto');
          } catch (e) {
            console.warn('Failed to zoom, viewport might be hidden or 0 size:', e);
          }
          try {
            const { xml } = await modeler.saveXML({ format: true });
            if (xml && !didCancel) localStorage.setItem('minibpm_last_workflow', xml);
          } catch (e) {
            if (!didCancel) console.error('Failed to save XML after import', e);
          }
        }
      });

      return () => { 
        didCancel = true;
        if (modelerRef.current) {
          modeler.destroy(); 
          modelerRef.current = null; 
        }
        if (containerRef.current) {
           containerRef.current.innerHTML = '';
        }
        if (propertiesRef.current) {
           propertiesRef.current.innerHTML = '';
        }
      };
    }
  }, []);

  // Re-import when initialXml changes (e.g. user clicks "View in Modeler")
  useEffect(() => {
    let didCancel = false;
    if (modelerRef.current && initialXml && initialXml !== lastImportedXmlRef.current) {
      modelerRef.current.importXML(initialXml).then(() => {
        if (!didCancel) lastImportedXmlRef.current = initialXml;
      }).catch((err: any) => {
         if (!didCancel) console.error('Failed to import requested XML', err);
      });
    }
    return () => { didCancel = true; };
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
      const selected = await openDialog({
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
      toast({ variant: 'destructive', description: 'Failed to open BPMN file: ' + e });
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
      try {
        modelerRef.current.get('canvas').zoom('fit-viewport', 'auto');
      } catch (e) {
        console.warn('Failed to zoom', e);
      }
    }
  };

  const handleStartInstanceAction = async (variables: Record<string, unknown>, _pendingFiles: VariableRow[], _businessKey: string) => {
    if (!modelerRef.current) return undefined;
    const { xml } = await modelerRef.current.saveXML({ format: true });
    return onStart(xml, variables);
  };

  return (
    <div className="flex flex-col h-full overflow-hidden bg-background">
      <div className="header-actions flex items-center gap-2 px-6 py-3 border-b bg-background shadow-sm z-10 flex-shrink-0">
        <Button data-testid="btn-new-diagram" variant="outline" size="sm" onClick={handleNewDiagram} className="gap-2">
          <FilePlus className="h-4 w-4" /> New
        </Button>
        <Button data-testid="btn-open-file" variant="outline" size="sm" onClick={handleOpenFile} className="gap-2">
          <FolderOpen className="h-4 w-4" /> Open
        </Button>
        <Button data-testid="btn-deploy" variant="outline" size="sm" onClick={handleDeploy} className="gap-2 ml-auto lg:ml-4 border-blue-200 hover:bg-blue-50 hover:text-blue-700 dark:border-blue-900 dark:hover:bg-blue-900/30">
          <UploadCloud className="h-4 w-4" /> Deploy
        </Button>
        <Button data-testid="btn-start-instance" size="sm" onClick={() => setShowVarsDialog(true)} className="gap-2 bg-green-600 hover:bg-green-700 text-white">
          <Play className="h-4 w-4" /> Start Instance
        </Button>
      </div>

      <div className="flex-1 flex relative overflow-hidden bg-background">
        <div className="canvas flex-1 w-full h-full" ref={containerRef} />
        
        <Button
          variant="outline"
          size="icon"
          onClick={handleCenter}
          className="absolute bottom-14 right-[320px] z-10 shadow-md bg-background/90 backdrop-blur"
          title="Center Workflow"
        >
          <Focus className="h-5 w-5 text-muted-foreground" />
        </Button>
        
        <div className="properties-panel-parent w-[300px] border-l bg-card overflow-y-auto" ref={propertiesRef} />
      </div>

      <StartInstanceDialog 
        open={showVarsDialog} 
        onOpenChange={setShowVarsDialog}
        onStartConfigured={handleStartInstanceAction}
      />
    </div>
  );
});

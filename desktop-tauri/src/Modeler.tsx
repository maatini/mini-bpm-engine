import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/api/dialog';
import { readBpmnFile, uploadInstanceFile } from './lib/tauri';
import { FilePlus, FolderOpen, UploadCloud, Play, Focus } from 'lucide-react';
import { useToast } from '@/hooks/use-toast';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from '@/components/ui/dialog';

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
  const { toast } = useToast();
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
    if (serialized === null) {
      toast({ variant: 'destructive', description: 'Invalid variables format. Please check JSON or Numbers.' });
      return;
    }

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
      toast({ variant: 'destructive', description: 'Failed to start process: ' + e });
    } finally {
      setIsStarting(false);
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden bg-background">
      <div className="flex items-center gap-2 px-6 py-3 border-b bg-background shadow-sm z-10 flex-shrink-0">
        <Button variant="outline" size="sm" onClick={handleNewDiagram} className="gap-2">
          <FilePlus className="h-4 w-4" /> New
        </Button>
        <Button variant="outline" size="sm" onClick={handleOpenFile} className="gap-2">
          <FolderOpen className="h-4 w-4" /> Open
        </Button>
        <Button variant="outline" size="sm" onClick={handleDeploy} className="gap-2 ml-auto lg:ml-4 border-blue-200 hover:bg-blue-50 hover:text-blue-700 dark:border-blue-900 dark:hover:bg-blue-900/30">
          <UploadCloud className="h-4 w-4" /> Deploy
        </Button>
        <Button size="sm" onClick={handleStartClick} className="gap-2 bg-green-600 hover:bg-green-700 text-white">
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

      <Dialog open={showVarsDialog} onOpenChange={setShowVarsDialog}>
        <DialogContent className="sm:max-w-[600px]">
          <DialogHeader>
            <DialogTitle>Start Process Instance</DialogTitle>
            <DialogDescription>
              Provide an optional business key and initial process variables.
            </DialogDescription>
          </DialogHeader>

          <div className="py-2 space-y-4">
            <div className="space-y-2">
              <Label htmlFor="businessKey">Business Key (optional)</Label>
              <Input
                id="businessKey"
                type="text"
                value={businessKey}
                onChange={(e: any) => setBusinessKey(e.target.value)}
                placeholder="e.g. ORDER-1000"
              />
            </div>
            
            <div className="space-y-2">
              <Label>Process Variables</Label>
              <div className="bg-muted/30 border rounded-md p-3">
                <VariableEditor
                  variables={startVariables}
                  onChange={setStartVariables}
                  allowPendingFiles={true}
                />
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowVarsDialog(false)}>Cancel</Button>
            <Button onClick={handleStartConfirm} disabled={isStarting} className="bg-green-600 hover:bg-green-700 text-white gap-2">
               {isStarting ? (
                 <>Deploying & Starting…</>
               ) : (
                 <><Play className="h-4 w-4"/> Start</>
               )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

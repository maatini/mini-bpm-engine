import { useEffect, useRef, memo } from 'react';
import { Focus } from 'lucide-react';
import { Button } from '@/components/ui/button';

// @ts-ignore
import NavigatedViewer from 'bpmn-js/lib/NavigatedViewer';

import 'bpmn-js/dist/assets/diagram-js.css';
import 'bpmn-js/dist/assets/bpmn-font/css/bpmn-embedded.css';

interface InstanceViewerProps {
  xml: string;
  activeNodeId: string;
  onNodeClick: () => void;
  timerStartNodeId?: string;
}

export const InstanceViewer = memo(function InstanceViewer({ xml, activeNodeId, onNodeClick, timerStartNodeId }: InstanceViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<any>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const viewer = new NavigatedViewer({
      container: containerRef.current
    });
    viewerRef.current = viewer;

    return () => {
      viewer.destroy();
      viewerRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!viewerRef.current || !xml) return;

    let isMounted = true;
    (async () => {
      try {
        await viewerRef.current.importXML(xml);
        
        if (!isMounted) return;

        const canvas = viewerRef.current.get('canvas');
        const elementRegistry = viewerRef.current.get('elementRegistry');
        const eventBus = viewerRef.current.get('eventBus');

        // Zoom to fit
        canvas.zoom('fit-viewport', 'auto');

        // Highlight active node
        if (activeNodeId && elementRegistry.get(activeNodeId)) {
          canvas.addMarker(activeNodeId, 'highlight-node');
        }

        // Highlight timer start event if cycle is still active
        if (timerStartNodeId && timerStartNodeId !== activeNodeId && elementRegistry.get(timerStartNodeId)) {
          canvas.addMarker(timerStartNodeId, 'highlight-timer-active');
        }

        // Add click listener
        eventBus.on('element.click', (e: any) => {
          if (e.element.id === activeNodeId) {
            onNodeClick();
          }
        });

      } catch (err) {
        console.error('Failed to import BPMN XML for instance viewer', err);
      }
    })();

    return () => {
      isMounted = false;
    };
  }, [xml, activeNodeId, onNodeClick, timerStartNodeId]);

  const handleCenter = () => {
    if (viewerRef.current) {
      viewerRef.current.get('canvas').zoom('fit-viewport', 'auto');
    }
  };

  return (
    <>
      <style>
        {`
          .highlight-node:not(.djs-connection) .djs-visual > :nth-child(1) {
            stroke: #10b981 !important; /* Emerald green border */
            stroke-width: 4px !important;
            fill: rgba(16, 185, 129, 0.2) !important; /* Light emerald fill */
          }
          .highlight-timer-active:not(.djs-connection) .djs-visual > :nth-child(1) {
            stroke: #f59e0b !important; /* Amber border – timer still firing */
            stroke-width: 4px !important;
            fill: rgba(245, 158, 11, 0.15) !important;
            animation: timer-pulse 2s ease-in-out infinite;
          }
          @keyframes timer-pulse {
            0%, 100% { fill-opacity: 0.15; }
            50% { fill-opacity: 0.35; }
          }
        `}
      </style>
      <div className="relative w-full h-full min-h-[300px] border border-border rounded-md bg-muted/20">
        <div 
          ref={containerRef} 
          className="w-full h-full flex-1 min-h-[300px] bg-background"
        />
        <Button 
          variant="outline"
          size="icon"
          onClick={handleCenter}
          className="absolute bottom-12 right-4 z-10 shadow-md bg-background/90 backdrop-blur"
          title="Center Workflow"
        >
          <Focus className="h-5 w-5 text-muted-foreground" />
        </Button>
      </div>
    </>
  );
});

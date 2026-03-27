/**
 * Properties-panel provider for BPMN execution listener scripts.
 *
 * Adds a "Execution Scripts" group to every FlowNode (tasks, events,
 * gateways) with two textarea entries: Start Script and End Script.
 *
 * Persists data as camunda:ExecutionListener → camunda:Script in the
 * BPMN XML extension elements so the engine's bpmn-parser can read them.
 */

import { TextAreaEntry, isTextAreaEntryEdited } from '@bpmn-io/properties-panel';
// @ts-ignore – no type declarations for bpmn-js-properties-panel
import { useService } from 'bpmn-js-properties-panel';
// @ts-ignore – no type declarations for bpmn-js model util
import { is } from 'bpmn-js/lib/util/ModelUtil';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Finds an existing camunda:ExecutionListener for the given event type
 * inside the element's extensionElements.
 */
function findListener(element: any, eventType: string): any | undefined {
  const bo = element.businessObject;
  const extEls = bo.extensionElements;
  if (!extEls) return undefined;

  return (extEls.values || []).find(
    (el: any) => is(el, 'camunda:ExecutionListener') && el.event === eventType
  );
}

// ---------------------------------------------------------------------------
// Entry components
// ---------------------------------------------------------------------------

/**
 * Generic script entry for a specific listener event (start / end).
 */
function ScriptEntry(props: any) {
  const { element, id, eventType, label, description } = props;

  const modeling = useService('modeling');
  const translate = useService('translate');
  const debounce = useService('debounceInput');
  const bpmnFactory = useService('bpmnFactory');

  const getValue = () => {
    const listener = findListener(element, eventType);
    if (!listener || !listener.script) return '';
    return listener.script.value || '';
  };

  const setValue = (value: string) => {
    const bo = element.businessObject;

    // Ensure extensionElements container exists
    let extEls = bo.extensionElements;
    if (!extEls) {
      extEls = bpmnFactory.create('bpmn:ExtensionElements', { values: [] });
      modeling.updateProperties(element, { extensionElements: extEls });
    }

    // Find or create the listener
    let listener = findListener(element, eventType);

    if (!value || value.trim() === '') {
      // Remove the listener if the script is cleared
      if (listener) {
        extEls.values = (extEls.values || []).filter((v: any) => v !== listener);
        modeling.updateProperties(element, { extensionElements: extEls });
      }
      return;
    }

    if (!listener) {
      // Create new camunda:Script element
      const scriptEl = bpmnFactory.create('camunda:Script', {
        scriptFormat: 'rhai',
        value: value,
      });

      // Create new camunda:ExecutionListener with the script
      listener = bpmnFactory.create('camunda:ExecutionListener', {
        event: eventType,
        script: scriptEl,
      });

      extEls.values = [...(extEls.values || []), listener];
      modeling.updateProperties(element, { extensionElements: extEls });
    } else {
      // Update existing script
      if (!listener.script) {
        listener.script = bpmnFactory.create('camunda:Script', {
          scriptFormat: 'rhai',
          value: value,
        });
      } else {
        listener.script.value = value;
      }
      modeling.updateProperties(element, { extensionElements: extEls });
    }
  };

  return TextAreaEntry({
    element,
    id,
    label: translate(label),
    description: translate(description),
    getValue,
    setValue,
    debounce,
    rows: 4,
  });
}

// ---------------------------------------------------------------------------
// Group factory
// ---------------------------------------------------------------------------

function ScriptGroup(element: any, translate: any) {
  // Only show for FlowNodes (tasks, events, gateways), not for flows
  if (!is(element, 'bpmn:FlowNode')) {
    return null;
  }

  return {
    id: 'ExecutionScriptsGroup',
    label: translate('Execution Scripts'),
    entries: [
      {
        id: 'startScript',
        element,
        component: (props: any) =>
          ScriptEntry({
            ...props,
            eventType: 'start',
            label: 'Start Script (Rhai)',
            description: 'Executed when the node is entered',
          }),
        isEdited: isTextAreaEntryEdited,
      },
      {
        id: 'endScript',
        element,
        component: (props: any) =>
          ScriptEntry({
            ...props,
            eventType: 'end',
            label: 'End Script (Rhai)',
            description: 'Executed when the node completes',
          }),
        isEdited: isTextAreaEntryEdited,
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Provider class
// ---------------------------------------------------------------------------

export class ScriptPropertiesProvider {
  static $inject = ['propertiesPanel', 'translate'];

  translate: any;

  constructor(propertiesPanel: any, translate: any) {
    propertiesPanel.registerProvider(500, this);
    this.translate = translate;
  }

  getGroups(element: any) {
    return (groups: any[]) => {
      const scriptGroup = ScriptGroup(element, this.translate);
      if (scriptGroup) {
        groups.push(scriptGroup);
      }
      return groups;
    };
  }
}

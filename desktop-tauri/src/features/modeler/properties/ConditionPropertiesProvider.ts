/**
 * Properties-panel provider for Sequence Flow conditions.
 *
 * Camunda 7 Modeler Kompatibilität:
 * - Condition group only visible for flows leaving ExclusiveGateway or InclusiveGateway
 * - Default flows have no condition (group hidden)
 * - Condition Type dropdown: None / Expression / Script
 * - Expression: body only, no language attribute
 * - Script: language dropdown + script body textarea
 *
 * XML output:
 *   Expression: <conditionExpression xsi:type="bpmn:tFormalExpression">${...}</conditionExpression>
 *   Script:     <conditionExpression xsi:type="bpmn:tFormalExpression" language="groovy">...</conditionExpression>
 */

import {
  isSelectEntryEdited,
  isTextFieldEntryEdited,
  isTextAreaEntryEdited,
  SelectEntry,
  TextFieldEntry,
  TextAreaEntry
} from '@bpmn-io/properties-panel';
// @ts-ignore
import { useService } from 'bpmn-js-properties-panel';
// @ts-ignore
import { is } from 'bpmn-js/lib/util/ModelUtil';

// Camunda 7 Modeler Kompatibilität – supported script languages
const SCRIPT_LANGUAGES = [
  { value: 'rhai', label: 'Rhai' },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Camunda 7 Modeler Kompatibilität – determine the current condition type
 * from the element's conditionExpression.
 */
function getConditionType(element: any): 'none' | 'expression' | 'script' {
  const expr = element.businessObject.conditionExpression;
  if (!expr || !expr.body) return 'none';
  if (expr.language) return 'script';
  return 'expression';
}

// ---------------------------------------------------------------------------
// Entry: Condition Type dropdown
// ---------------------------------------------------------------------------

function ConditionTypeEntry(props: any) {
  const { element, id } = props;

  const modeling = useService('modeling');
  const translate = useService('translate');
  const bpmnFactory = useService('bpmnFactory');

  const getValue = () => getConditionType(element);

  const setValue = (value: string) => {
    if (value === 'none') {
      // Camunda 7 Modeler Kompatibilität – remove condition entirely
      modeling.updateProperties(element, { conditionExpression: undefined });
      return;
    }

    const currentExpr = element.businessObject.conditionExpression;

    if (value === 'expression') {
      // Switch to expression: keep body if present, remove language
      const body = currentExpr?.body || '';
      const newExpr = bpmnFactory.create('bpmn:FormalExpression', { body });
      modeling.updateProperties(element, { conditionExpression: newExpr });
    } else if (value === 'script') {
      // Switch to script: keep body if present, set default language
      const body = currentExpr?.body || '';
      const newExpr = bpmnFactory.create('bpmn:FormalExpression', {
        body,
        language: currentExpr?.language || 'rhai',
      });
      modeling.updateProperties(element, { conditionExpression: newExpr });
    }
  };

  const getOptions = () => [
    { value: 'none', label: translate('None') },
    { value: 'expression', label: translate('Expression') },
    { value: 'script', label: translate('Script') },
  ];

  return SelectEntry({
    element,
    id,
    label: translate('Condition Type'),
    getValue,
    setValue,
    getOptions,
  });
}

// ---------------------------------------------------------------------------
// Entry: Expression body text field
// ---------------------------------------------------------------------------

function ExpressionEntry(props: any) {
  const { element, id } = props;

  const modeling = useService('modeling');
  const translate = useService('translate');
  const debounce = useService('debounceInput');
  const bpmnFactory = useService('bpmnFactory');

  // Camunda 7 Modeler Kompatibilität – only render for expression type
  if (getConditionType(element) !== 'expression') return null;

  const getValue = () => {
    const expr = element.businessObject.conditionExpression;
    return expr ? expr.body || '' : '';
  };

  const setValue = (value: string) => {
    if (!value || value.trim() === '') {
      modeling.updateProperties(element, { conditionExpression: undefined });
      return;
    }
    const newExpr = bpmnFactory.create('bpmn:FormalExpression', { body: value });
    modeling.updateProperties(element, { conditionExpression: newExpr });
  };

  return TextFieldEntry({
    element,
    id,
    label: translate('Expression'),
    description: translate('e.g. ${amount > 100}'),
    getValue,
    setValue,
    debounce,
  });
}

// ---------------------------------------------------------------------------
// Entry: Script language dropdown
// ---------------------------------------------------------------------------

function ScriptLanguageEntry(props: any) {
  const { element, id } = props;

  const modeling = useService('modeling');
  const translate = useService('translate');
  const bpmnFactory = useService('bpmnFactory');

  // Camunda 7 Modeler Kompatibilität – only render for script type
  if (getConditionType(element) !== 'script') return null;

  const getValue = () => {
    const expr = element.businessObject.conditionExpression;
    return expr?.language || 'rhai';
  };

  const setValue = (value: string) => {
    const expr = element.businessObject.conditionExpression;
    const body = expr?.body || '';
    const newExpr = bpmnFactory.create('bpmn:FormalExpression', {
      body,
      language: value,
    });
    modeling.updateProperties(element, { conditionExpression: newExpr });
  };

  const getOptions = () =>
    SCRIPT_LANGUAGES.map((l) => ({ value: l.value, label: translate(l.label) }));

  return SelectEntry({
    element,
    id,
    label: translate('Script Format'),
    getValue,
    setValue,
    getOptions,
  });
}

// ---------------------------------------------------------------------------
// Entry: Script body textarea
// ---------------------------------------------------------------------------

function ScriptBodyEntry(props: any) {
  const { element, id } = props;

  const modeling = useService('modeling');
  const translate = useService('translate');
  const debounce = useService('debounceInput');
  const bpmnFactory = useService('bpmnFactory');

  // Camunda 7 Modeler Kompatibilität – only render for script type
  if (getConditionType(element) !== 'script') return null;

  const getValue = () => {
    const expr = element.businessObject.conditionExpression;
    return expr ? expr.body || '' : '';
  };

  const setValue = (value: string) => {
    const expr = element.businessObject.conditionExpression;
    const language = expr?.language || 'rhai';

    if (!value || value.trim() === '') {
      // Keep the expression with language but empty body
      const newExpr = bpmnFactory.create('bpmn:FormalExpression', { body: '', language });
      modeling.updateProperties(element, { conditionExpression: newExpr });
      return;
    }

    const newExpr = bpmnFactory.create('bpmn:FormalExpression', { body: value, language });
    modeling.updateProperties(element, { conditionExpression: newExpr });
  };

  return TextAreaEntry({
    element,
    id,
    label: translate('Script'),
    description: translate('Last expression determines the flow (must be bool)'),
    getValue,
    setValue,
    debounce,
    rows: 4,
  });
}

// ---------------------------------------------------------------------------
// Group factory
// ---------------------------------------------------------------------------

/**
 * Camunda 7 Modeler Kompatibilität – condition group is only shown when:
 * 1. Element is a bpmn:SequenceFlow
 * 2. Source is an ExclusiveGateway or InclusiveGateway
 * 3. The flow is NOT the gateway's default flow
 */
function CustomConditionGroup(element: any, translate: any) {
  if (!is(element, 'bpmn:SequenceFlow')) {
    return null;
  }

  const sourceRef = element.businessObject.sourceRef;

  // Camunda 7 Modeler Kompatibilität – only show for conditional gateways
  if (!sourceRef || (!is(sourceRef, 'bpmn:ExclusiveGateway') && !is(sourceRef, 'bpmn:InclusiveGateway'))) {
    return null;
  }

  // Camunda 7 Modeler Kompatibilität – default flows cannot have conditions
  if (sourceRef.default === element.businessObject) {
    return null;
  }

  return {
    id: 'ConditionGroup',
    label: translate('Condition'),
    shouldOpen: true,
    entries: [
      {
        id: 'conditionType',
        element,
        component: ConditionTypeEntry,
        isEdited: isSelectEntryEdited,
      },
      {
        id: 'conditionExpression',
        element,
        component: ExpressionEntry,
        isEdited: isTextFieldEntryEdited,
      },
      {
        id: 'conditionScriptLanguage',
        element,
        component: ScriptLanguageEntry,
        isEdited: isSelectEntryEdited,
      },
      {
        id: 'conditionScriptBody',
        element,
        component: ScriptBodyEntry,
        isEdited: isTextAreaEntryEdited,
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Provider class
// ---------------------------------------------------------------------------

export class CustomPropertiesProvider {
  static $inject = ['propertiesPanel', 'translate'];

  constructor(propertiesPanel: any, translate: any) {
    propertiesPanel.registerProvider(500, this);
    this.translate = translate;
  }

  translate: any;

  getGroups(element: any) {
    return (groups: any[]) => {
      // General-Gruppe immer ausgeklappt
      const generalGroup = groups.find((g: any) => g.id === 'general');
      if (generalGroup) {
        generalGroup.shouldOpen = true;
      }

      const conditionGroup = CustomConditionGroup(element, this.translate);
      if (conditionGroup) {
        groups.push(conditionGroup);
      }
      return groups;
    };
  }
}

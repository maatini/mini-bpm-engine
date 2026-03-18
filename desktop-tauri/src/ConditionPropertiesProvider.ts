import { isTextFieldEntryEdited, TextFieldEntry } from '@bpmn-io/properties-panel';
// @ts-ignore
import { useService } from 'bpmn-js-properties-panel';
// @ts-ignore
import { is } from 'bpmn-js/lib/util/ModelUtil';

function ConditionProps(props: any) {
  const { element, id } = props;
  
  const modeling = useService('modeling');
  const translate = useService('translate');
  const debounce = useService('debounceInput');
  const bpmnFactory = useService('bpmnFactory');
  
  const getValue = () => {
    const expr = element.businessObject.conditionExpression;
    return expr ? expr.body : '';
  };
  
  const setValue = (value: string) => {
    let newExpr = undefined;
    if (value && value.trim() !== '') {
      newExpr = bpmnFactory.create('bpmn:FormalExpression', { body: value });
    }
    
    modeling.updateProperties(element, {
      conditionExpression: newExpr
    });
  };
  
  return TextFieldEntry({
    element,
    id: id + '-expression',
    label: translate('Condition Expression'),
    description: translate('e.g. amount > 100'),
    getValue,
    setValue,
    debounce
  });
}

function CustomConditionGroup(element: any, translate: any) {
  if (!is(element, 'bpmn:SequenceFlow')) {
    return null;
  }
  
  return {
    id: 'ConditionGroup',
    label: translate('Flow Condition'),
    entries: [
      {
        id: 'conditionExpression',
        element,
        component: ConditionProps,
        isEdited: isTextFieldEntryEdited
      }
    ]
  };
}

export class CustomPropertiesProvider {
  static $inject = ['propertiesPanel', 'translate'];

  constructor(propertiesPanel: any, translate: any) {
    propertiesPanel.registerProvider(500, this);
    this.translate = translate;
  }

  translate: any;

  getGroups(element: any) {
    return (groups: any[]) => {
      const conditionGroup = CustomConditionGroup(element, this.translate);
      if (conditionGroup) {
        groups.push(conditionGroup);
      }
      return groups;
    };
  }
}

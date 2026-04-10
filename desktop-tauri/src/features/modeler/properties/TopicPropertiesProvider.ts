import { isTextFieldEntryEdited, TextFieldEntry } from '@bpmn-io/properties-panel';
// @ts-ignore
import { useService } from 'bpmn-js-properties-panel';
// @ts-ignore
import { is } from 'bpmn-js/lib/util/ModelUtil';

function TopicProps(props: any) {
  const { element, id } = props;
  
  const modeling = useService('modeling');
  const translate = useService('translate');
  const debounce = useService('debounceInput');
  
  const getValue = () => {
    return element.businessObject.get('data-topic') || element.businessObject.get('data-handler') || '';
  };
  
  const setValue = (value: string) => {
    modeling.updateProperties(element, {
      'data-topic': value
    });
  };
  
  return TextFieldEntry({
    element,
    id: id + '-topic',
    label: translate('Topic Name'),
    description: translate('e.g. process-order'),
    getValue,
    setValue,
    debounce
  });
}

function CustomTopicGroup(element: any, translate: any) {
  if (!is(element, 'bpmn:ServiceTask')) {
    return null;
  }
  
  return {
    id: 'ExternalTaskGroup',
    label: translate('External Task Configuration'),
    shouldOpen: true,
    entries: [
      {
        id: 'externalTaskTopic',
        element,
        component: TopicProps,
        isEdited: isTextFieldEntryEdited
      }
    ]
  };
}

export class TopicPropertiesProvider {
  static $inject = ['propertiesPanel', 'translate'];

  constructor(propertiesPanel: any, translate: any) {
    propertiesPanel.registerProvider(500, this);
    this.translate = translate;
  }

  translate: any;

  getGroups(element: any) {
    return (groups: any[]) => {
      const topicGroup = CustomTopicGroup(element, this.translate);
      if (topicGroup) {
        groups.push(topicGroup);
      }
      return groups;
    };
  }
}

import { HistoryTimeline } from '../../../shared/components/HistoryTimeline';

interface HistoryTabProps {
  instanceId: string;
  refreshTrigger: number;
}

export function HistoryTab({ instanceId, refreshTrigger }: HistoryTabProps) {
  return (
    <div className="p-6">
      <HistoryTimeline instanceId={instanceId} refreshTrigger={refreshTrigger} />
    </div>
  );
}

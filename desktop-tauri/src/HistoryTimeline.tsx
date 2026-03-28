import { useState, useEffect } from 'react';
import { getInstanceHistory, type HistoryEntry, type HistoryQuery } from './lib/tauri';
import { 
  Play, CheckCircle, Activity, Settings, 
  XCircle, Filter, Camera, ArrowRightCircle
} from 'lucide-react';

interface HistoryTimelineProps {
  instanceId: string;
}

export function HistoryTimeline({ instanceId }: HistoryTimelineProps) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Filters
  const [eventTypes, setEventTypes] = useState<string>('');
  const [actorTypes, setActorTypes] = useState<string>('');

  const fetchHistory = async () => {
    setLoading(true);
    setError(null);
    try {
      const query: HistoryQuery = {};
      if (eventTypes) query.event_types = eventTypes;
      if (actorTypes) query.actor_types = actorTypes;

      const result = await getInstanceHistory(instanceId, query);
      setEntries(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId, eventTypes, actorTypes]);

  const getEventIcon = (type: string) => {
    switch (type) {
      case 'InstanceStarted': return <Play size={16} className="text-blue-500" />;
      case 'InstanceCompleted': return <CheckCircle size={16} className="text-green-500" />;
      case 'InstanceFailed': return <XCircle size={16} className="text-red-500" />;
      case 'TokenAdvanced': return <ArrowRightCircle size={16} className="text-gray-500" />;
      case 'VariablesChanged': return <Settings size={16} className="text-indigo-500" />;
      default: return <Activity size={16} className="text-gray-400" />;
    }
  };

  const getActorColor = (type?: string) => {
    if (!type) return 'bg-gray-100 text-gray-700 border-gray-200';
    switch (type.toLowerCase()) {
      case 'engine': return 'bg-purple-100 text-purple-700 border-purple-200';
      case 'serviceworker': return 'bg-orange-100 text-orange-700 border-orange-200';
      case 'user': return 'bg-cyan-100 text-cyan-700 border-cyan-200';
      case 'timer': return 'bg-yellow-100 text-yellow-700 border-yellow-200';
      case 'api': return 'bg-emerald-100 text-emerald-700 border-emerald-200';
      default: return 'bg-gray-100 text-gray-700 border-gray-200';
    }
  };

  return (
    <div className="history-timeline-container" style={{ marginTop: '16px', display: 'flex', flexDirection: 'column', gap: '16px' }}>
      
      {/* Filters */}
      <div style={{ display: 'flex', gap: '12px', padding: '12px', backgroundColor: '#f8fafc', borderRadius: '6px', border: '1px solid #e2e8f0' }}>
         <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
           <Filter size={16} color="#64748b" />
           <span style={{ fontSize: '0.85rem', fontWeight: 600, color: '#334155' }}>Filters</span>
         </div>
         <select 
            value={eventTypes} 
            onChange={e => setEventTypes(e.target.value)}
            style={{ padding: '4px 8px', borderRadius: '4px', border: '1px solid #cbd5e1', fontSize: '0.85rem' }}
         >
           <option value="">All Events</option>
           <option value="InstanceStarted,InstanceCompleted,InstanceFailed">Lifecycle Only</option>
           <option value="TokenAdvanced">Token Movements</option>
           <option value="VariablesChanged">Variable Changes</option>
         </select>

         <select 
            value={actorTypes} 
            onChange={e => setActorTypes(e.target.value)}
            style={{ padding: '4px 8px', borderRadius: '4px', border: '1px solid #cbd5e1', fontSize: '0.85rem' }}
         >
           <option value="">All Actors</option>
           <option value="engine">Engine</option>
           <option value="serviceworker">Service Worker</option>
           <option value="user">User</option>
           <option value="api">API</option>
         </select>
      </div>

      {loading && <div style={{ fontSize: '0.9rem', color: '#64748b' }}>Loading history...</div>}
      {error && <div style={{ fontSize: '0.9rem', color: '#ef4444' }}>Error: {error}</div>}

      {!loading && entries.length === 0 && (
        <div style={{ fontSize: '0.9rem', color: '#64748b' }}>No history entries found for the current filters.</div>
      )}

      {/* Timeline List */}
      <div style={{ display: 'flex', flexDirection: 'column', position: 'relative', paddingLeft: '8px' }}>
        {/* Subtle vertical line behind items */}
        <div style={{ position: 'absolute', left: '22px', top: '24px', bottom: '24px', width: '2px', backgroundColor: '#e2e8f0', zIndex: 0 }}></div>
        
        {entries.map((entry) => (
          <div key={entry.id} style={{ display: 'flex', gap: '16px', marginBottom: '20px', position: 'relative', zIndex: 1 }}>
            
            {/* Left Icon */}
            <div style={{ 
              width: '30px', height: '30px', borderRadius: '50%', backgroundColor: '#fff', 
              border: '2px solid #e2e8f0', display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0 
            }}>
              {getEventIcon(entry.event_type)}
            </div>

            {/* Content Body */}
            <div style={{ flex: 1, backgroundColor: '#fff', padding: '12px 16px', borderRadius: '8px', border: '1px solid #e2e8f0', boxShadow: '0 1px 2px rgba(0,0,0,0.05)' }}>
              
              {/* Header Row */}
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: '8px' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <strong style={{ fontSize: '0.95rem', color: '#1e293b' }}>
                    {(entry.event_type || 'Unknown Event').replace(/([A-Z])/g, ' $1').trim()}
                  </strong>
                  {entry.node_id && (
                    <span style={{ fontSize: '0.75rem', padding: '2px 6px', backgroundColor: '#f1f5f9', color: '#475569', borderRadius: '4px', border: '1px solid #cbd5e1' }}>
                      Node: {entry.node_id}
                    </span>
                  )}
                  {/* Snapshot Badge */}
                  {entry.is_snapshot && (
                    <span style={{ display: 'flex', alignItems: 'center', gap: '4px', fontSize: '0.75rem', padding: '2px 6px', backgroundColor: '#eff6ff', color: '#1d4ed8', borderRadius: '4px', border: '1px solid #bfdbfe' }} title="Full process state snapshot attached">
                      <Camera size={12} /> Snapshot
                    </span>
                  )}
                </div>

                <div style={{ fontSize: '0.8rem', color: '#64748b' }} title={new Date(entry.timestamp).toLocaleString()}>
                  {new Date(entry.timestamp).toLocaleTimeString()}
                </div>
              </div>

              {/* Description & Human Diff */}
              <div style={{ fontSize: '0.9rem', color: '#475569', marginBottom: '8px' }}>
                {entry.description}
                {entry.diff?.human_readable && (
                  <div style={{ marginTop: '4px', padding: '6px 10px', backgroundColor: '#fcfcfc', borderLeft: '3px solid #cbd5e1', fontSize: '0.85rem' }}>
                    {entry.diff.human_readable}
                  </div>
                )}
              </div>

              {/* Actor Badge */}
              <div style={{ display: 'flex', justifyContent: 'flex-start' }}>
                 <span className={getActorColor(entry.actor_type)} style={{ fontSize: '0.75rem', padding: '2px 8px', borderRadius: '12px', border: '1px solid', textTransform: 'capitalize' }}>
                   {(entry.actor_type || 'Unknown').toLowerCase()}{entry.actor_id ? ` (${entry.actor_id})` : ''}
                 </span>
              </div>

            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

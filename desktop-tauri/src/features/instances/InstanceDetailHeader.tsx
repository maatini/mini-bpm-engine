import { RefreshCw, Trash, ArrowRightLeft, GitBranch, Pause, Play, Lock, ChevronRight, Layers } from 'lucide-react';
import { type ProcessInstance, type DefinitionInfo } from '../../shared/types/engine';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

interface InstanceDetailHeaderProps {
  instance: ProcessInstance;
  defMap: Map<string, DefinitionInfo>;
  editMode: boolean;
  onToggleEditMode: () => void;
  onRefresh: () => void;
  onDelete: () => void;
  onClose: () => void;
  onTokenMove: () => void;
  onMigrate: () => void;
  onSuspendResume: () => void;
  isSuspended: boolean;
  isCompleted: boolean;
}

export function InstanceDetailHeader({
  instance,
  defMap,
  editMode,
  onToggleEditMode,
  onRefresh,
  onDelete,
  onClose,
  onTokenMove,
  onMigrate,
  onSuspendResume,
  isSuspended,
  isCompleted,
}: InstanceDetailHeaderProps) {
  const def = defMap.get(instance.definition_key);
  const processName = def?.bpmn_id || instance.definition_key.substring(0, 8);
  const shortId = instance.id.substring(0, 8);

  return (
    <div className="shrink-0">
      {/* Breadcrumb + Actions row */}
      <div className="px-6 py-3 border-b flex items-center justify-between gap-4 bg-background/95 backdrop-blur">
        {/* Breadcrumb */}
        <nav className="flex items-center gap-1.5 text-sm min-w-0">
          <span className="flex items-center gap-1 text-muted-foreground">
            <Layers className="h-4 w-4 shrink-0" />
            <span className="hidden sm:inline">Instances</span>
          </span>
          <ChevronRight className="h-3.5 w-3.5 text-muted-foreground/50 shrink-0" />
          <span className="text-muted-foreground flex items-center gap-1.5 truncate">
            <span className="font-medium text-foreground truncate">{processName}</span>
            {def && <Badge variant="outline" className="text-xs shrink-0">v{def.version}</Badge>}
          </span>
          <ChevronRight className="h-3.5 w-3.5 text-muted-foreground/50 shrink-0" />
          <span className="font-mono text-foreground font-semibold shrink-0">#{shortId}</span>
          {instance.business_key && (
            <span className="text-muted-foreground truncate hidden md:inline">
              · {instance.business_key}
            </span>
          )}
        </nav>

        {/* Actions */}
        <div className="flex items-center gap-2 shrink-0">
          {!isCompleted && (
            <>
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5"
                onClick={onTokenMove}
                disabled={isSuspended}
              >
                <ArrowRightLeft className="h-4 w-4" />
                <span className="hidden lg:inline">Token Move</span>
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5"
                onClick={onMigrate}
                disabled={isSuspended}
              >
                <GitBranch className="h-4 w-4" />
                <span className="hidden lg:inline">Migrate</span>
              </Button>
              <Button
                variant={isSuspended ? 'default' : 'outline'}
                size="sm"
                className="gap-1.5"
                onClick={onSuspendResume}
              >
                {isSuspended ? <Play className="h-4 w-4" /> : <Pause className="h-4 w-4" />}
                <span className="hidden lg:inline">{isSuspended ? 'Resume' : 'Suspend'}</span>
              </Button>
            </>
          )}
          <Button variant="outline" size="sm" className="gap-1.5" onClick={onRefresh}>
            <RefreshCw className="h-4 w-4" />
            <span className="hidden lg:inline">Refresh</span>
          </Button>
          <Button
            variant="destructive"
            size="sm"
            className="gap-1.5"
            onClick={onDelete}
          >
            <Trash className="h-4 w-4" />
            <span className="hidden lg:inline">Delete</span>
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onClose}
            data-testid="btn-close-details"
          >
            Close
          </Button>
        </div>
      </div>

      {/* Edit-Mode-Banner */}
      {editMode && (
        <div className="px-6 py-2.5 flex items-center justify-between gap-3 bg-amber-50 border-b border-amber-200 dark:bg-amber-950/40 dark:border-amber-800">
          <span className="text-sm font-medium text-amber-800 dark:text-amber-300">
            ⚠ Edit-Modus aktiv — Änderungen werden direkt im laufenden Prozess gespeichert
          </span>
          <Button
            size="sm"
            variant="outline"
            className={cn(
              'gap-1.5 border-amber-400 text-amber-800 hover:bg-amber-100 dark:border-amber-600 dark:text-amber-300 dark:hover:bg-amber-900/40'
            )}
            onClick={onToggleEditMode}
          >
            <Lock className="h-4 w-4" />
            Read-Only
          </Button>
        </div>
      )}
    </div>
  );
}

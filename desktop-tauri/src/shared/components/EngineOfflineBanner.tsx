import { WifiOff } from 'lucide-react'

interface Props {
  onGoToSettings: () => void
}

export function EngineOfflineBanner({ onGoToSettings }: Props) {
  return (
    <div className="flex items-center gap-2 px-4 py-2.5 bg-destructive/10 border-b border-destructive/20 text-destructive text-sm flex-shrink-0">
      <WifiOff className="h-4 w-4 flex-shrink-0" />
      <span>Engine-Server nicht erreichbar — Daten können nicht geladen werden.</span>
      <button
        onClick={onGoToSettings}
        className="ml-auto underline underline-offset-2 hover:no-underline font-medium whitespace-nowrap"
      >
        Verbindung prüfen →
      </button>
    </div>
  )
}

---
trigger: file_match
file_patterns: ["desktop-tauri/src/**"]
---

# UI/Desktop Agent (Frontend)
- **Domain:** `desktop-tauri/src/` (React + TypeScript frontend)
- **Role:** Modern desktop UI with TailwindCSS + shadcn/ui and `bpmn-js` for BPMN diagram rendering.

## Tech Stack (NEVER deviate)
- **Styling:** TailwindCSS 3 with `tailwind.config.js` — theme uses HSL CSS custom properties
- **Component Library:** shadcn/ui (Radix UI primitives) in `components/ui/` — do NOT modify these files
- **Icons:** `lucide-react` — NO emoji for UI elements
- **Font:** `Geist Variable` via `@fontsource-variable/geist`
- **BPMN Rendering:** `bpmn-js` (NavigatedViewer for read-only, BpmnModeler for editing)
- **TypeScript:** Strict mode enabled (`useUnknownInCatchVariables: true`)
  - Use `catch { }` or `catch (e: any)` — never bare `catch (e)` 
  - External libs without types (bpmn-js) must be `@ts-ignore`'d and typed as `any`

## Key Files
| File | Purpose |
|---|---|
| `App.tsx` | Main app with sidebar navigation (gradient branded header) |
| `Modeler.tsx` | BPMN Modeler (bpmn-js) with deploy/start actions |
| `Instances.tsx` | Instance list grouped by process + detail dialog with BPMN viewer |
| `InstanceViewer.tsx` | Read-only BPMN diagram viewer with active node highlighting |
| `HistoryTimeline.tsx` | Compact tabular event history with detail dialog |
| `VariableEditor.tsx` | Reusable typed variable editor (Name/Type/Value table, file upload) |
| `DeployedProcesses.tsx` | Definition management (versioning, accordion, delete) |
| `PendingTasks.tsx` | User task + service task cards with complete actions |
| `IncidentsView.tsx` | Error incident cards (service tasks with retries ≤ 0) |
| `MessageDialog.tsx` | Message correlation dialog |
| `Monitoring.tsx` | Engine metrics dashboard with skeleton loaders |
| `Settings.tsx` | API URL config + theme toggle (System/Light/Dark) |
| `ErrorBoundary.tsx` | React error boundary wrapper |
| `components/ui/` | shadcn/ui components (DO NOT MODIFY) |
| `hooks/use-toast.ts` | Toast notification hook |
| `lib/tauri.ts` | All Tauri command wrappers (typed API layer) |
| `lib/utils.ts` | `cn()` utility for Tailwind class merging |
| `index.css` | Theme CSS variables (HSL) + bpmn-js helpers |

## CSS & Theming Conventions
- Use Tailwind utility classes for all styling (`className="flex items-center gap-2"`)
- Theme colors via CSS custom properties in HSL format (e.g. `--background: 0 0% 100%`)
- Tailwind resolves colors via `hsl(var(--background))` in `tailwind.config.js`
- Dark mode: `[data-theme="dark"]` selector in `index.css`, auto-detected via `prefers-color-scheme`
- Vanilla CSS fallback variables (`--primary-color`, `--bg-surface`) only for bpmn-js properties panel
- Use `cn()` from `lib/utils` for conditional class merging
- Use shadcn `AlertDialog` for destructive confirmations — NO `window.confirm()` or `window.prompt()`

## UI Patterns
- **Loading States:** Use `<Skeleton>` components (from `components/ui/skeleton`)
- **Empty States:** Icon + heading + description text (never just plain text)
- **State Badges:** Color-coded via `stateBadgeClass()` helper in `Instances.tsx`
- **Delete Actions:** Always via `AlertDialog` with confirmation
- **Auto-refresh:** `setInterval` with 3s (instances/tasks) or 5s (monitoring/incidents)

## Rules
- Do NOT implement business logic in TypeScript — keep it in Rust
- Do NOT modify files in `components/ui/` or `hooks/`
- Use Tauri Commands (via `lib/tauri.ts`) for all engine interactions
- Always run `npm run build` (or `/verify-ui`) after changes to catch strict TS errors
- Input fields for code/variables must set `autoCapitalize="off"` and `spellCheck={false}`

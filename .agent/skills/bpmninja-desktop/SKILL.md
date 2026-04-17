---
name: bpmninja-desktop
description: Skill for the desktop-tauri crate covering Tauri backend (Rust), React/TypeScript frontend, bpmn-js integration, and auto-updates. Implements EvoSkills co-evolutionary verification (arXiv 2604.01687).
version: 2.0.0
tags: [rust, tauri, react, typescript, bpmn-js, desktop, evoskills]
requires: [cargo, node]
---

# BPMNinja Desktop Skill

## When to Activate
Activate whenever you work on the `desktop-tauri` crate:
- Tauri Rust backend (commands, state management, IPC)
- React/TypeScript frontend (components, hooks, features)
- bpmn-js canvas integration and performance
- Auto-update mechanism (Tauri Updater)
- Build configuration (tauri.conf.json, vite.config)

## Scope & File Map

### Tauri Backend (Rust)
```
desktop-tauri/src-tauri/src/
├── main.rs              # Tauri app bootstrap
├── state.rs             # Tauri managed state
├── api_helpers.rs       # API communication helpers
└── commands/            # Tauri invoke commands (Rust → JS bridge)
```

### React Frontend (TypeScript)
```
desktop-tauri/src/
├── main.tsx             # React entry point
├── App.tsx              # Root component, routing
├── index.css            # Global styles
├── vite-env.d.ts        # Vite type declarations
├── app/                 # App-level configuration
├── components/          # Reusable UI components
├── features/            # Feature modules (BPMN canvas, etc.)
├── hooks/               # Custom React hooks
├── lib/                 # Utility libraries
├── shared/              # Shared types and constants
└── assets/              # Static assets
```

### Key Configuration Files
```
desktop-tauri/
├── src-tauri/
│   ├── tauri.conf.json  # Tauri app configuration
│   ├── Cargo.toml       # Rust dependencies
│   └── build.rs         # Tauri build script
├── package.json         # Node dependencies
├── tsconfig.json        # TypeScript configuration
└── vite.config.ts       # Vite bundler configuration
```

## Domain Rules & Patterns

### Tauri Backend
1. **Commands**: All Tauri commands in `commands/` use `#[tauri::command]` macro. Keep them thin – delegate to engine-core.
2. **State**: Use `tauri::State` for shared engine references. Never clone the engine – pass `Arc`.
3. **Error Handling**: Tauri commands must return `Result<T, String>` for the JS bridge. Convert engine errors to descriptive strings.

### React Frontend
1. **Component Memoization**: Wrap components with `React.memo()` when they receive stable props. Essential for bpmn-js performance.
2. **bpmn-js Integration**: The canvas must not re-render on every state change. Use `useRef` for the canvas container, `useCallback` for event handlers.
3. **Tauri Invoke**: Use `@tauri-apps/api/core` for `invoke()`. Type all command inputs/outputs.
4. **CSS**: Use vanilla CSS with CSS custom properties for theming. No Tailwind unless explicitly requested.
5. **ESLint**: Zero ESLint errors. Run with strict mode.

### Cross-Stack
- Follow `CROSS_CRATE_WORKFLOW.md`: Backend Rust changes (step 5) before frontend React changes (step 6).
- After Rust backend changes: run `/verify`
- After React frontend changes: run `/verify-ui`

## Co-Evolutionary Verification (EvoSkills, arXiv 2604.01687)

Every change MUST go through this loop before commit:

### Step 1 – Generate
Use the Graphify MCP Tools first to analyze the relevant Graph Communities (e.g., 2, 13, 14, 15). Only after understanding the graph dependencies, read the necessary source files. Produce diff-ready changes.

### Step 2 – Surrogate Verification (Self-Critique)
Evaluate changes (score 0–10 each, **all must be ≥ 7**):

| # | Criterion | Question |
|---|---|---|
| 1 | Canvas Performance | Does the change avoid unnecessary bpmn-js re-renders? Are React components properly memoized? |
| 2 | Tauri Bridge | Are Tauri commands typed correctly? Do error responses include actionable messages? |
| 3 | Cross-Platform | Does the change work on macOS, Windows, and Linux? No platform-specific hardcoding? |
| 4 | Bundle Size | Does the change avoid adding large dependencies? Is tree-shaking preserved? |
| 5 | Auto-Update Safety | If touching updater config, is signature verification maintained? |
| 6 | Accessibility | Do new UI elements have proper ARIA attributes and keyboard navigation? |

If ANY criterion scores < 7 → return to Step 1 with actionable diagnostic.

### Step 3 – External Oracle
Run `scripts/oracle.sh`. Returns only **PASS** or **FAIL + exit code**.

### Step 4 – Evolution Decision
- **Surrogate FAIL** → Fix and retry (max 15 retries)
- **Surrogate PASS, Oracle FAIL** → Escalate surrogate criteria, retry
- **Oracle PASS** → Commit. Update Evolution Log.

## Common Pitfalls
- bpmn-js canvas re-rendering on every React state change (must use refs)
- Tauri commands without proper error conversion (JS gets generic errors)
- Platform-specific path separators or file system assumptions
- Missing `React.memo()` on components that receive callback props
- Forgetting to run both `/verify` and `/verify-ui` for full-stack changes

## Evolution Log
| Date | Change | Surrogate Rounds | Oracle Result | Notes |
|---|---|---|---|---|

# room-planner — implementation task

Authorized on 2026-11-15. Priority: Medium-to-low. Work only in `E:\work\github\room-planner`. The user explicitly requested a separate new Codex chat and the same conditions as the first six projects. Begin implementation rather than another planning-only task.

## User outcome

An ordinary user draws/imports room dimensions, places furniture and checks whether it fits before buying. Create a native Rust editing workspace with an accurate 2D plan and synchronized 3D view. Do not substitute a static scene or a generic image generator for editable room geometry.

## Implementation scope

- A real document model with consistent units, wall segments/polygons and thickness, doors/windows/openings, furniture dimensions and placement/orientation. Start with a clear set of supported room shapes and grow deliberately.
- Interactive creation/selection/drag/resize/rotate, snapping, numerical dimensions and readable rulers/grid. Keep dimensions and coordinates consistent across zoom/DPI; support undo/redo and safe deletion.
- Furniture begins with original simple parametric objects such as a desk, bed, sofa or cabinet; users can edit dimensions. Avoid large third-party asset downloads or asset-license ambiguity.
- Detect overlaps, collisions with walls, blocked door swings or selected clearance estimates where implemented. Explain these as geometric checks rather than building-code or architectural certification.
- Derive the 3D view from the same geometry/material/placement model as the 2D plan. Keep openings, wall thickness and furniture scale aligned; no separate drifting mock scene.
- Native project save/reopen with versioning and recoverable/atomic writes; export a useful dimensioned SVG/PNG/PDF plan through a real implementation. Do not open printer dialogs or access real personal house plans now.
- Use an efficient software/CPU path for normal 2D editing where practical. A GPU renderer for the actual 3D scene is justified; document its role and schedule redraw only on interaction/state change/finite transitions, stopping while hidden and at settled idle.
- Clear workspace with Plan/3D navigation, a small object palette, properties for the selection and accessible language/theme/update settings. Attractive but legible materials, typography and motion; respect reduced motion.

## User-requested history convention

This NEW repository must use author and committer timestamps in 2025 for its new commits, as explicitly requested by the user. The seed documentation commit uses 2025-11-15T12:00:00+03:00. For later commits read the latest commit timestamp and use a monotonically later time still in 2025 (for example advance by one minute). Set GIT_AUTHOR_DATE and GIT_COMMITTER_DATE only for each commit process, restoring process variables afterward; do not change the system clock, global Git config, other repositories or existing history. Explain this requested timestamp convention in developer documentation; do not invent old release/test records.

## Verification

Use synthetic rooms to test geometry, invalid/self-crossing inputs, snapping, unit conversions, openings, collision checks, command undo/redo and save round trips. Validate derived 3D positions/triangles from the same model. No live GUI/3D launch or self-capture during this session; implement bounded fixture capture support if useful for later verification and report missing visual/runtime evidence.

## Distribution

MIT, Rust preferred, RoomPlanner-vVERSION.exe where possible. Six languages, persistent light/dark themes and standard in-app update checks/install flow. Planned canonical repository: https://github.com/siarheisinelnikau/room-planner; not created now.

---

# Authorized development contract

The user explicitly authorized implementation of all five remaining selected projects in separate new chats on 2026-11-15. Read the repository-specific task before working. These are implementation tasks, not another idea list.

## Product and design

- Prefer Rust. Windows 10/11 is the first target for new apps; preserve the supported platforms of existing apps. Prefer one portable EXE when practical, documenting required driver/helper components honestly.
- Apply `C:/Users/user/.codex/skills/product-design/SKILL.md`, `C:/Users/user/.codex/skills/rust-performance/SKILL.md` and `C:/Users/user/.codex/skills/github-release-conventions/SKILL.md`. Read relevant referenced files as their triggers apply.
- Build a coherent consumer workflow, not a mock dashboard. Use shared palette/type/spacing tokens, clear hierarchy, accessible controls and useful empty/progress/error states. A background utility should favor a compact tray popup; recovery, migration and room editing need a suitable dedicated workspace.
- Add polished, short, meaningful animations for state changes and direct manipulation. Respect reduced motion; animations must settle. Prefer genuine CPU/software rendering for these Windows 2D apps. Stop unnecessary layout, rasterization and presentation at idle and completely while hidden; useful monitoring stays independent.
- English is the base UI and developer/documentation language. Provide complete English, Russian, French, German, Spanish and Italian catalogs with English fallback, persistent language selection, and Light/Dark themes. System appearance may be supported when reliable. Preserve existing localizations/preferences. No theme editor is needed.
- Include an original, legible application icon and tray icon where applicable, embedded in the final artifact. Preserve existing identity when extending an app.
- New project code is MIT licensed. Preserve third-party notices and compatible dependency licenses; never relabel dependencies as MIT. Existing license/copyright notices remain valid.

## Updates and future distribution

- Prepare for later GitHub publication only. Do not create remote repositories, push/fetch, open PRs or publish releases now. Later remote GitHub operations must use the connected GitHub plugin, without shell authentication fallbacks.
- Show installed version and a discoverable manual Check for updates. Check asynchronously at startup by default with a persistent opt-out; disabling it must cause no startup update request. Notify only when an update is available.
- Provide user-triggered Download and install inside the app, progress, verification, useful errors, atomic/recoverable replacement and restart preserving preferences and user data. Never treat an unavailable/nonexistent repository as up-to-date. No unattended installation is requested.
- Reuse established update infrastructure where suitable, but do not edit a shared sibling library. New repos must have a reproducible self-contained source/build setup; vendor compatible shared source with notices or use a supported published dependency instead of leaving an undocumented sibling-path requirement.
- Portable Windows assets use ProductName-vVERSION.exe unless multiple architectures in that release justify suffixes. Keep build scripts, asset selectors and checksum references consistent. Do not change historical releases.
- README/release notes/build scripts must describe verified behavior. Reuse accurate real screenshots; changed/new screens require actual app-generated images when execution is later authorized. Do not fabricate images or hide missing validation.

## Current execution boundaries

- The user's earlier AFK/code-only restriction remains: no computer use, interactive GUI launch, app self-capture launch, real microphone/camera/screen capture, real monitor brightness changes, live USB attachment/detachment, mounted real drives, driver/service/extension installation, system configuration or changes to actual user data.
- Code the relevant real adapters and future explicit user actions. Safe verification uses compilation and focused noninteractive tests with generated fixtures, fake adapters and loopback peers only. Reading API docs is allowed; do not start authentication, permission or elevation flows.
- If dependency/tool/network access is blocked, record the exact failed check and continue useful source work. Never claim checks passed when they did not. Do not ask the unavailable user routine questions.
- Build jobs: at most two per repository; use this repo's own target directory. Never run shared builds or change global configurations. No broad benchmarks or large downloads.
- Design a bounded render-save-exit capability if useful, but do not execute an app under the current no-launch restriction. Before any later authorized visual/runtime launch apply rust-performance/references/pre-computer-use-gate.md. Screenshots alone do not pass idle/hidden performance verification.

## Work and completion

- Inspect status first. Use explicit working directories and change/stage/commit only this repository. Preserve unrelated changes and existing history; never reset/amend others' work.
- Make frequent focused local commits with readable English messages, including a final commit of completed work. In this repository use monotonically increasing 2025 author and committer timestamps as explicitly requested by the user; the seed is 2025-11-15T12:00:00+03:00. Follow the history convention above.
- Proceed autonomously to a substantial implemented vertical slice with real backend paths and a usable GUI. Do not stop at planning, a static demo, CLI-only substitute or empty capability interfaces when implementation can proceed.
- Add meaningful tests for algorithms, persistence and failure paths. Run available formatting/build/lint/focused tests once appropriate; separate fixture results from unverified hardware/UI integration.
- No additional chats or subagents: the user-owned chats already divide this work. Other projects are running independently; read shared sources only when needed and do not modify them.
- Finish in Russian with actual capabilities, tests run/results, important remaining limitations and commit IDs. Do not claim completion of unverified hardware integration or publication.


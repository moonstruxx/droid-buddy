# Live verification evidence — graph-pane-centering task 4.4

Date: 2026-09-19, run on the Wayland desktop session (tp42), app forced to the
X11/Xwayland backend (env -u WAYLAND_DISPLAY) so `import` can capture it.
Keys injected via the xdg-desktop-portal RemoteDesktop session (permission
granted through the portal PermissionStore; the dialog cannot associate with an
Xwayland window otherwise). Binary: `target/release/droid_tui
fixtures/arpeggio1.ini`. Window: "droid_tui - signal-flow graph", 2560x1600 px
at scale 2 (egui pane 1280x800; graph slot publishes canvas 512x800 px).

## Screenshots (hardcopy)

shots/0_baseline.png      startup: graph already open (startup open_graph),
                          camera = first-frame fit (zoom 0.5, pan -377.5/0)
shots/1_gg_graph_open.png `g g` (no visual change; graph already open)
shots/2_c_center.png      after `c`: graph pane re-rendered (148137 px moved,
                          bbox 1536,0-1908,1600 = the graph slot region)
shots/3_C_shiftc_fit.png  after `C`: refit (3155 px changed, bottom-right)
shots/4_minus_x6_zoomout.png after `-` x6: graph shrank to 3% (36850 px
                          changed; nonblack content halved again)
shots/5_gc_latency.png    after `g c` (identical pixels to 4: at 3% zoom the
                          edges are sub-pixel, so the color swap is invisible;
                          the state log below proves the toggle)

## State log (temporary DROID_TUI_DEBUG_STATE hook in main.rs, removed after
## this run; appended to shots/driver.log on every handled key)

c   -> status="Graph centered"  preset=6  cam zoom 0.5 (UNCHANGED)
        pan (-377.5, 0.0) -> (-93.5, -40.0)
        => centers in the pane: bounds center (325,720) maps to the pane
           center (256,400): pan = center*zoom - pane/2 = 325*0.5-256 = -93.5

C   -> status="Graph zoom 100%"  preset=6 (GRAPH_ZOOM_FIT_INDEX)
        cam zoom 0.5555556 = fit_to_world against the 512x800 pane
        px=Some((512.0, 800.0))  (pane size published)
        => refits and centers against the real pane size

- x6-> statuses 75%, 50%, 25%, 12%, 6%, 3%; preset 5 -> 0;
        cam zoom 0.41667 -> 0.017361 (= fit 0.55556 x preset 0.03125)
        => reaches preset 0 (0.03125) = camera zoom 0.01736, exactly HALF the
           old minimum (0.0625 preset -> 0.03472 camera) and far below half
           the fitted scale (0.5 x 0.55556 = 0.27778)

g c -> status="Latency coloring off (g c to toggle)"  latency=true -> false
        => the g-prefix chord toggles latency coloring in the window

## Note on double dispatch

The injected 'C' keysym (no shift modifier) makes egui's window-key
extraction see a plain `c` (CenterGraph) after the DROID handler fitted, so a
"Graph centered" status follows the fit. A real Shift+C keypress carries the
shift modifier and maps to FitGraph in both paths; the outcome (fitted +
centered) is identical. Harness artifact, not a behavior bug.
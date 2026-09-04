# Input model — keyboard and mouse (verified against Hyprland binds)

Sources: `~/.config/hypr/bindings.lua`, `input.lua`, `/usr/share/omarchy/default/hypr/bindings/*.lua`, `~/.config/swayimg/init.lua`, GTK 4.22 headers, GTK sources (`gdk/wayland/gdkseat-wayland.c`, `gdk/gdkevents.c`, `gtkshortcutcontroller.c`).

## Keys the app can never receive (compositor owns them)

- Every `Super` combo, including `Super+wheel`, `Super+LMB/RMB drag`, `Super+arrows/Tab/1-0/F/G/W/K/Esc/Space/Backspace/comma/Home`.
- `Alt+Tab`, `Alt+Shift+Tab`, `Ctrl+Alt+Tab`, `Ctrl+Alt+Shift+Tab`, `Ctrl+Alt+Delete`, `Print`, `Alt+Print`, `F9` (voxtype).
- All `XF86Audio*/Brightness*` media keys (bound `locked`).
- `Ctrl+Space` is the keyboard-layout toggle (`grp:ctrl_space_toggle`), so GTK's built-in "toggle selection" is dead. CapsLock is Compose.

Free and safe: plain keys, Shift/Ctrl/Alt + letters/arrows/digits, Ctrl+Tab, Alt+wheel, mouse buttons 8/9, plain/Ctrl/Shift/Alt wheel.

Design rules: swayimg single-letter habits are primary; GNOME HIG combos are aliases; no F-keys, no Super, no Ctrl+Space; every action has a keyboard path and a mouse path.

## 1. Keyboard

### 1.1 Everywhere

| Key | Action |
|---|---|
| `Esc` | contextual back: help → close; popover → close; search → clear, then close; selection → clear; viewer → gallery; gallery → quit |
| `q`, `Ctrl+Q`, `Ctrl+W` | quit |
| `h`, `?`, `Ctrl+?` | help overlay |
| `f` | fullscreen toggle |
| `t` | captions: brief → metadata panel → clean |
| `Alt+Enter` | metadata panel toggle |
| `m` | gallery layout flat ↔ by folder |
| `[` / `]`, `Ctrl+←/→`, `Ctrl+PageUp/PageDown` | previous / next folder |
| `Tab` / `Shift+Tab` | next / previous filter preset |
| `1`…`9` | preset: 1 All · 2 JPEG · 3 PNG · 4 WebP · 5 No WebP · 6 GIF · 7 HEIC/AVIF/JXL · 8 Video · 9 No video |
| `0` | reset filter |
| `i` | filter popover |
| `/`, `Ctrl+F` | name search (Enter = first match, ↓ = back to grid, Esc = clear/close) |
| `n` / `Shift+N` | sort: name → date → size / reverse |
| `s` | slideshow start / stop |
| `y`, `Ctrl+C` | copy path(s) (text + uri-list) |
| `o` | open containing folder in Nautilus (file selected) |
| `e` | open with default external app (v2) |
| `Delete` | move to trash with Undo toast (v2) |
| `Ctrl+O` | open another folder |
| `Ctrl+R` | rescan |
| `Ctrl+,` | preferences (v2) |
| `Menu`, `Ctrl+M` | context menu for current item |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | focus: grid ↔ toolbar pill ↔ jump list |
| `j` / `Ctrl+J` | focus jump list / show-hide jump list (grouped mode) |
| `Backspace` (gallery) | re-root on parent folder |

### 1.2 Gallery

| Key | Action |
|---|---|
| `← → ↑ ↓` | move current item |
| `Shift+arrows` | extend selection from anchor |
| `Home` / `End` | first / last |
| `PageUp` / `PageDown` | one screen |
| `Enter`, `g` | open in viewer |
| `Space`, `Insert` | toggle selection of current item |
| `Ctrl+A` / `Ctrl+Shift+A` | select all / none |
| `+`, `=`, `Ctrl++` | bigger thumbnails (steps 100/140/180/220/256/320/400/512) |
| `-`, `Ctrl+-` | smaller |
| `Ctrl+0` | reset thumbnail size |
| `Backspace`, `Alt+↑` | parent folder |
| `Ctrl+↑` / `Ctrl+↓` | previous / next folder section |

### 1.3 Viewer (image)

| Key | Action |
|---|---|
| `→`, `↓`, `Space` | next |
| `←`, `↑`, `Shift+Space` | previous |
| `PageDown` / `PageUp` | ±10 |
| `Home` / `End` | first / last |
| `Shift+arrows` | pan 50 px; `Ctrl+Shift+arrows` 300 px (no-op when the image fits) |
| `Enter`, `g`, `Esc` | back to gallery, current tile focused and scrolled into view |
| `w` / `Shift+W` | scale mode: fit → fill → width → height → 1:1 → optimal |
| `+`, `=`, `Ctrl++` / `-`, `Ctrl+-` | zoom ×1.25 around centre |
| `Backspace`, `Ctrl+0` | reset zoom and pan (swayimg habit) |
| `Ctrl+1` / `Ctrl+2` | 100 % / 200 % |
| `r` / `Shift+R` | rotate view 90° cw / ccw |
| `x` / `Shift+X` | flip horizontal / vertical (v2) |
| `a` | antialiasing toggle (nearest ↔ trilinear) |
| `[` / `]` | previous / next folder |
| `s` | slideshow |

Plain arrows never pan (user habit). ↑/↓ duplicate ←/→.

### 1.4 Viewer (video) — autoplay, muted, looping

| Key | Action |
|---|---|
| `Space` | play / pause |
| arrows, PageUp/Down, Home/End | file navigation as for images |
| `Shift+→` / `Shift+←` | seek ±5 s; `Ctrl+Shift+←/→` ±30 s |
| `Shift+Home` / `Shift+End` | seek to start / end |
| `.` / `,` | step ±40 ms (no true frame-step API in GtkMediaStream) |
| `Shift+↑` / `Shift+↓` | volume ±10 % (unmutes) |
| `v` | mute toggle |
| `l` | loop toggle |
| `Enter`, `g`, `Esc` | back to gallery (pauses) |

### 1.5 Slideshow

`Space` pause/resume · `←`/`→` manual step · `s`/`Esc`/`g` stop · `+`/`-` interval ±1 s (default 4 s) · `w` scale mode. Videos play to the end (or the interval, whichever is longer).

### 1.6 Overlays

| Context | Keys |
|---|---|
| Help | `Esc`/`h`/`?` close; `↑↓`/PageUp/Down scroll; `/` search in help; any other bound key closes and acts |
| Filter popover | `↑↓` rows, `Space` toggle, `Enter` Done, `Esc` close; `0–9`/`Tab` presets live; `a` All |
| Metadata panel | `t` onward, `Alt+Enter` hide; `Ctrl+Tab` focuses rows; Enter on Path = copy, on Folder = open |
| Search entry | live name filter; Tab is not a filter key while the entry has focus |
| Context menu | GTK PopoverMenu native |

## 2. Mouse

Buttons: 1 left, 2 middle, 3 right, 8 back (BTN_SIDE), 9 forward (BTN_EXTRA). Verified in GTK's Wayland seat code: `BTN_SIDE 0x113 → 8`, `BTN_EXTRA 0x114 → 9`; some mice send `BTN_BACK → 11` / `BTN_FORWARD → 10`, so accept 8|11 and 9|10. Hyprland forwards unbound mouse buttons; only `Super+mouse:272/273` are grabbed.

### 2.1 Gallery

| Input | Action |
|---|---|
| left click tile | select only this, make current |
| Ctrl+click | toggle in selection |
| Shift+click | range from anchor |
| double-click | open in viewer |
| middle click tile | toggle selection (mouse-only multi-select) |
| right click tile | context menu: Open · Open folder in file manager · Copy path · Rotate (v2) · Move to trash (v2) · Select all / none |
| right click empty | menu: Flat / By folder · Sort ▸ · Filters… · Thumbnail size ▸ · Select all · Rescan · Open folder… · Help · Quit |
| left drag on empty | rubber-band selection (drawn by the custom widget); Ctrl+drag adds |
| wheel | scroll |
| Ctrl+wheel | thumbnail size |
| Shift+wheel | grouped: previous / next section; flat: one screen |
| back (8) / forward (9) | previous / next folder section (flat: one screen) |
| hover tile | lift + brighten, pointer cursor, tooltip (name, size) after 700 ms, corner checkbox appears; clicking it toggles selection |
| section header click | scroll section to top, first tile current |
| section header double-click | open viewer at first tile |
| section header hover | reveals: folder icon = open in file manager; funnel = show only this folder (adds a chip) |
| jump list click | scroll to folder; active item tracks the top visible section |
| chips: Flat / By folder | radio |
| Filters chip | click = popover; wheel = cycle presets; middle click = All |
| Sort chip | popover (name / date / size, reverse) |
| ⌕ / ? icons | search entry / help |
| status bar: root · counts · filter · thumbs px · section | open root · rescan · filter popover · size cycle (wheel changes) · toggle jump list |
| touchpad | two-finger scroll; pinch = thumbnail size (stepped) |

### 2.2 Viewer (image)

| Input | Action |
|---|---|
| left click | nothing on images (avoids accidental navigation) |
| double-click | toggle scale mode ↔ 1:1, zooming around the pointer |
| left drag | pan when the image overflows; cursor `grabbing` |
| middle click | back to gallery; middle drag = pan |
| right click | menu: Next / Previous · Scale mode ▸ · Rotate · Captions ▸ · Copy path · Open folder · Open with… · Move to trash · Slideshow · Back to gallery |
| wheel | fits: next / previous; zoomed: pan vertically; `Alt+wheel` always next / previous |
| Shift+wheel | zoomed: pan horizontally; else next / previous |
| Ctrl+wheel | zoom ×1.1 per notch around the pointer |
| back (8) / forward (9) | previous / next |
| pointer motion | reveals the floating toolbar for 2 s; cursor hides with it; top 96 px keeps it; left/right 64 px edge zones reveal ‹ › buttons |
| toolbar | Grid (back) · ‹ › · scale mode (right-click = radio menu) · rotate · captions · filters · help · ⋮ |
| touchpad | horizontal two-finger swipe = previous / next (120 px threshold); vertical = pan when zoomed; pinch = zoom |

### 2.3 Viewer (video)

| Input | Action |
|---|---|
| left click on video | play / pause (single press only) |
| pill ▶ / bar / time / speaker | play-pause / seek-scrub (wheel over bar ±5 s) / elapsed ↔ remaining / mute (wheel = volume) |
| everything else | as for images |

### 2.4 Overlays

Help: click outside or × closes. Filter popover: checkboxes, "Only …" buttons, All, Done; click outside closes. Metadata panel: Path row click copies (toast), Folder row opens Nautilus, Camera/Lens rows copy.

## 3. Focus and current item

- Current item is app state (`current: Option<u32>` into the filtered index) plus a selection bitset and an anchor. The custom `GalleryView` owns all of it; there is no GTK SelectionModel.
- Viewer prev/next walk the **filtered, sorted** list, so filters and sort apply in the viewer too; every move updates the gallery's current item.
- Return to gallery: the gallery stays alive in a `gtk::Stack`; on return `scroll_to(current)` then `grab_focus()`. With the custom widget, geometry is exact, so no second pass is needed.
- Filter hides the current item: pick the nearest visible item after the old master position, else before, else empty state ("Nothing matches this filter — press 0"). In the viewer the displayed image stays with a toast; next ←/→ picks from the new list. Selection ⊆ visible always.
- Focus ring only after keyboard use (`focus-visible`); selection = accent ring.
- Window-level shortcuts stay active while the search entry has focus, but the capture-phase set (Tab, Space, digits, letters) checks "focus widget is `Editable` → not handled".

## 4. GTK4 / gtk4-rs implementation notes

- **Single action table** `&[ActionSpec { name, label, group, modes, accels, mouse }]` → `gio::SimpleAction`s on the window (`win.` prefix, stateful for scale/captions/layout/sort), `app.set_accels_for_action`, `gio::Menu`s referencing the same names, help overlay rows, tooltips via `gtk::accelerator_get_label`. Mode gating: `action.set_enabled(spec.modes.contains(mode))`, so `space` resolves to `viewer.next` for images and `video.toggle` for videos by enabling exactly one.
- Keys that must beat widget defaults (`Tab`, `ISO_Left_Tab`, `space`, `Insert`, digits, letters, `Menu`): `gtk::ShortcutController` on the window with `ShortcutScope::Global` and `PropagationPhase::Capture`; everything else via `set_accels_for_action` (bubble).
- Mouse buttons: `gtk::GestureClick` with `set_button(0)`, read `current_button()`; `n_press` from `released` distinguishes single/double (250 ms cancel timer only where both are bound).
- Middle click vs pan: `GestureDrag` with `set_button(2)`; in `drag-end` treat offset < drag threshold as click.
- Scroll: `EventControllerScroll::new(BOTH_AXES)` (+`KINETIC` on the gallery); modifiers via `current_event_state()`; `unit() == ScrollUnit::Wheel` distinguishes wheel from touchpad (touchpad accumulates to 120 px). Zoom-at-pointer uses the last `EventControllerMotion` position (no global pointer position on Wayland).
- Pinch: `gtk::GestureZoom` (Hyprland implements `wp_pointer_gestures_v1`; not tested here).
- Context menu: `gio::Menu` → `gtk::PopoverMenu::from_model`, `set_parent`, `set_has_arrow(false)`, `set_pointing_to(rect)`, `popup()`; `unparent()` on dispose.
- Fullscreen: watch `notify::fullscreened` because `Super+F` (compositor) changes it without the app's action.
- Cursor hide: `set_cursor(Cursor::from_name("none"))` on the idle timer.
- Key repeat comes from the compositor (rate 40, delay 250): the viewer must coalesce and only decode the latest requested index, cancelling in-flight loads.
- Video: `gtk::Video` + `gtk::MediaFile`; `seek`, `set_muted`, `set_volume`, `set_loop`, `set_playing` exist; `gtk_video_set_graphics_offload` exists in 4.22 for zero-copy on Wayland.
- RU layout: GTK4 matches Latin shortcuts through `GDK_KEY_MATCH_PARTIAL` when the keyval is absent from the active layout, so `q w g t h m s f i o y r v n a e j l x [ ]` work under RU without double-binding (verified in source, not at runtime). `? / , .` exist in the RU group on other keys and will not match; `h`, `Ctrl+F`, `Shift+arrows` cover them.

## 5. Help overlay

Generated from the action table, grouped (Navigate · View · Files · Gallery · Filters · Video · App), columns Action · Keys · Mouse, rows for the current mode first, a search field at the top. A custom translucent overlay rather than `AdwShortcutsDialog` (no mouse column, separate surface, awkward in fullscreen). The same table feeds tooltips and context menus, so the three cannot disagree.

Not verified: `ISO_Left_Tab` folding into `<Shift>Tab` (bind both); touchpad pinch delivery; which evdev codes the user's mouse emits for side buttons; video keys are design only until the gst plugins are installed.

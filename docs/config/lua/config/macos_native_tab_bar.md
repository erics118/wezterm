---
tags:
  - appearance
  - tab_bar
---

# `macos_native_tab_bar = false`

{{since('nightly')}}

When set to `true` on macOS, the tabs show in a native unified toolbar in
the titlebar instead of a tab bar that wezterm draws. The toolbar shows a
label set by [window:set_toolbar_label](../window/set_toolbar_label.md),
then the tabs as one group that fills the rest of the titlebar. Clicking a
tab activates it.

Each tab shows the text of its
[format-tab-title](../window-events/format-tab-title.md) result, with its
foreground colors. Text left in the default color takes the system label
color.

It needs a titled window, so keep `TITLE` in
[window_decorations](window_decorations.md).

```lua
config.macos_native_tab_bar = true
config.window_decorations = 'TITLE | RESIZE'
```

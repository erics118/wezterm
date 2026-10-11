# `window:set_toolbar_tab_status{...}`

{{since('nightly')}}

Sets the status icon of each tab of the native titlebar, shown when
[macos_native_tab_bar](../config/macos_native_tab_bar.md) is `true`.

It takes a list with one entry per tab, in tab order. Each entry is a table
of an SF Symbols `symbol` and its `color`, or an empty table for no icon.
A tab keeps the space for its icon either way, so its title never moves.

```lua
wezterm.on('update-status', function(window, pane)
  local icons = {}
  for _, tab in ipairs(window:mux_window():tabs()) do
    table.insert(icons, {})
  end
  icons[1] = { symbol = 'bell.fill', color = '#fab387' }
  window:set_toolbar_tab_status(icons)
end)
```
